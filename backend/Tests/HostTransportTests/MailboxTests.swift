import Foundation
import HostTransport
import Testing

@Test func shutdownDoesNotWaitForFrontendReturnOrRequestCapacity() async {
  let box = Mailbox()
  for _ in 0..<16 { #expect(box.submit(Data("pending".utf8)) == .accepted) }
  // Terminal notification can precede the host's wait; later UI completion cannot replace it.
  await Task.detached { box.requestExit(129) }.value
  box.requestExit(0)
  var exits: [Int32] = []
  for await code in box.exits { exits.append(code) }
  #expect(exits == [129])
  box.close()
  #expect(box.receive(capacity: 100) == .closed)
}

@Test func saturationReservesRepliesAndNeverBlocksClose() async {
  let box = Mailbox()
  let data = Data("request".utf8)
  for _ in 0..<16 { #expect(box.submit(data) == .accepted) }
  #expect(box.submit(data) == .busy)
  for _ in 0..<16 { #expect(box.complete(data)) }
  #expect(!box.complete(data))
  #expect(box.receive(capacity: 1) == .required(data.count))
  #expect(box.submit(data) == .busy)
  #expect(box.receive(capacity: 100) == .message(data))
  // The stream still holds 16 undrained requests, so it cannot accept more yet.
  #expect(box.submit(data) == .busy)
  box.close()
  #expect(box.submit(data) == .closed)
  #expect(box.receive(capacity: 100) == .closed)
  #expect(!box.complete(data))
  var drained = 0
  for await _ in box.requests { drained += 1 }
  #expect(drained == 16)
}

@Test func snapshotsCoalesceWithoutStarvingReplies() {
  let box = Mailbox()
  let reply = Data("reply".utf8)
  #expect(box.submit(reply) == .accepted)
  #expect(box.complete(reply))
  #expect(box.publish("player", Data("old".utf8)))
  #expect(box.publish("player", Data("new".utf8)))
  #expect(box.receive(capacity: 100) == .message(reply))
  #expect(box.receive(capacity: 100) == .message(Data("new".utf8)))
  #expect(box.receive(capacity: 100) == .idle)
  for index in 0..<16 { #expect(box.publish(String(index), reply)) }
  #expect(!box.publish("overflow", reply))
  box.close()
}
