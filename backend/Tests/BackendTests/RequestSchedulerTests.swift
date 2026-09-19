import Foundation
import MusicContract
import Testing

@testable import Backend

private func message(_ id: UInt64, _ command: Command) throws -> Data {
  try Wire.encode(Request(version: 1, id: id, command: command))
}

@MainActor @Test func cancelledWorkStillCountsUntilItActuallyFinishes() throws {
  let scheduler = RequestScheduler { _ in .ack(.init(message: "done")) }
  for id: UInt64 in 1...16 {
    scheduler.submit(try message(id, .snapshot(.init()))) { _ in }
    scheduler.submit(try message(id + 100, .cancel(.init(requestId: id)))) { _ in }
  }
  var response: Event?
  scheduler.submit(try message(1000, .snapshot(.init()))) { response = $0 }
  guard case .failure(let failure) = response?.event else {
    Issue.record("Expected backpressure")
    return
  }
  #expect(failure.code == .busy)
  scheduler.close()
}

@MainActor @Test func readsProceedWhileMutationsRemainOrdered() async throws {
  let gate = AsyncStream<Void>.makeStream()
  let done = AsyncStream<Event>.makeStream()
  var mutations: [UInt64] = []
  var published: [UInt64] = []
  let scheduler = RequestScheduler { request in
    if request.id == 1 {
      for await _ in gate.stream { break }
    }
    if request.id == 2 { #expect(published == [3, 1]) }
    if request.id != 3 { mutations.append(request.id) }
    return .ack(.init(message: String(request.id)))
  }
  let reply: RequestScheduler.Reply = {
    published.append($0.id!)
    done.continuation.yield($0)
  }
  scheduler.submit(try message(1, .control(.init(action: .pause))), reply: reply)
  scheduler.submit(try message(2, .control(.init(action: .stop))), reply: reply)
  scheduler.submit(try message(3, .snapshot(.init())), reply: reply)
  var iterator = done.stream.makeAsyncIterator()
  #expect(await iterator.next()?.id == 3)
  #expect(mutations.isEmpty)
  gate.continuation.yield(())
  gate.continuation.finish()
  let first = await iterator.next()
  let second = await iterator.next()
  #expect([first?.id, second?.id] == [1, 2])
  #expect([first?.sequence, second?.sequence] == [2, 3])
  #expect(mutations == [1, 2])
  scheduler.close()
}

@MainActor @Test func cancellationSettlesExactlyOnceAndAllowsReuse() async throws {
  let gate = AsyncStream<Void>.makeStream()
  let started = AsyncStream<Void>.makeStream()
  var responses: [Event] = []
  let scheduler = RequestScheduler { request in
    if case .search = request.command {
      started.continuation.yield(())
      for await _ in gate.stream { break }
    }
    return .ack(.init(message: "완료"))
  }
  scheduler.submit(
    try message(1, .search(.init(query: "slow", source: .catalog, kind: .song, offset: 0)))
  ) {
    responses.append($0)
  }
  var start = started.stream.makeAsyncIterator()
  await start.next()
  scheduler.submit(try message(2, .cancel(.init(requestId: 1)))) { responses.append($0) }
  #expect(responses.map(\.id) == [1, 2])
  guard case .failure(let error) = responses[0].event else {
    Issue.record("Expected cancellation")
    return
  }
  #expect(error.code == .cancelled)
  let reused = try message(1, .snapshot(.init()))
  let next = await withCheckedContinuation { continuation in
    scheduler.submit(reused) { continuation.resume(returning: $0) }
  }
  #expect(next.id == 1)
  scheduler.close()
  gate.continuation.yield(())
  gate.continuation.finish()
  #expect(responses.count == 2)
}

@MainActor @Test func malformedCommandsRetainOnlyValidBoundedEnvelopeIdentities() {
  let scheduler = RequestScheduler { _ in
    Issue.record("Malformed request reached service")
    return .ack(.init(message: "unexpected"))
  }
  let examples: [(String, UInt64?, ErrorCode)] = [
    (#"{"version":1,"id":42,"command":{"type":"unknown","data":{}}}"#, 42, .invalidRequest),
    (#"{"version":1,"id":43,"command":{"type":"seek","data":{"seconds":"bad","entryId":"a"}}}"#, 43, .invalidRequest),
    (#"{"version":2,"id":44,"command":{"type":"snapshot","data":{}}}"#, 44, .unsupportedVersion),
    (#"{"version":1,"id":0,"command":{"type":"snapshot","data":{}}}"#, nil, .invalidRequest),
    (#"{"version":1,"id":"42","command":{"type":"unknown","data":{}}}"#, nil, .invalidRequest),
    (#"{"version":1,"id":42,"command":broken}"#, nil, .invalidRequest),
    ("{\"id\":42,\"command\":" + String(repeating: "[", count: 25) + "0"
      + String(repeating: "]", count: 25) + "}", nil, .invalidRequest),
    (#"{"id":42,"command":""# + String(repeating: "x", count: 8193) + "\"}", nil, .invalidRequest),
    (String(repeating: " ", count: maxMessageBytes + 1), nil, .invalidRequest),
  ]
  for (json, id, code) in examples {
    var event: Event?
    scheduler.submit(Data(json.utf8)) { event = $0 }
    #expect(event?.id == id)
    guard case .failure(let failure) = event?.event else {
      Issue.record("Expected correlated failure")
      continue
    }
    #expect(failure.code == code)
    #expect(!failure.retryable)
  }
  scheduler.close()
}

@MainActor @Test func duplicateEnvelopesNeverConsumeAnAcceptedRequestIdentity() async throws {
  let gate = AsyncStream<Void>.makeStream()
  let started = AsyncStream<Void>.makeStream()
  let completed = AsyncStream<Event>.makeStream()
  let scheduler = RequestScheduler { _ in
    started.continuation.yield(())
    for await _ in gate.stream { break }
    return .ack(.init(message: "original"))
  }
  let original = try message(42, .snapshot(.init()))
  scheduler.submit(original) { completed.continuation.yield($0) }
  var starting = started.stream.makeAsyncIterator()
  await starting.next()
  for duplicate in [
    original,
    Data(#"{"version":1,"id":42,"command":{"type":"unknown","data":{}}}"#.utf8),
    Data(#"{"version":2,"id":42,"command":{"type":"snapshot","data":{}}}"#.utf8),
  ] {
    var failure: Event?
    scheduler.submit(duplicate) { failure = $0 }
    #expect(failure?.id == nil)
    guard case .failure = failure?.event else { Issue.record("Expected duplicate failure"); continue }
  }
  gate.continuation.yield(())
  gate.continuation.finish()
  var replies = completed.stream.makeAsyncIterator()
  let reply = await replies.next()
  #expect(reply?.id == 42)
  guard case .ack(let ack) = reply?.event else { Issue.record("Original result was lost"); return }
  #expect(ack.message == "original")
  scheduler.close()
}
