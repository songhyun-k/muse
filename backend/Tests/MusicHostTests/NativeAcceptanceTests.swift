import Backend
import Foundation
import MusicContract
import Testing

@testable import MusicHost

@MainActor @Test(arguments: ["success", "move", "remove", "pause", "seek", "mode-on", "mode-off", "resume", "stop"])
func nativeAcceptanceRejectsAcknowledgementsWithoutControlEffects(_ ignored: String) async throws {
  let service = try DemoService()
  defer { service.close() }
  var id: UInt64 = 0
  @MainActor func call(_ command: Command) async throws -> Notice {
    let action: String
    switch command {
    case .queueMove: action = "move"
    case .queueRemove: action = "remove"
    case .seek: action = "seek"
    case .mode(let mode): action = mode.shuffle ? "mode-on" : "mode-off"
    case .control(let control):
      switch control.action {
      case .pause: action = "pause"
      case .toggle: action = "resume"
      case .stop: action = "stop"
      default: action = ""
      }
    default: action = ""
    }
    if action == ignored { return .ack(.init(message: "ignored")) }
    id += 1
    let data = try Wire.encode(Request(version: 1, id: id, command: command))
    let event = await withCheckedContinuation { continuation in
      service.submit(data) { continuation.resume(returning: $0) }
    }
    if case .failure(let failure) = event.event { throw failure }
    return event.event
  }
  guard case .snapshot(let snapshot) = try await call(.snapshot(.init())),
    let song = snapshot.player.current
  else { throw TestFailure.missingFixture }
  guard case .player(let player) = try await call(.play(.init(
    items: [song.ref, song.ref, song.ref], startIndex: 0, placement: .replace, shuffle: false))),
    let entry = player.currentEntryId,
    case .queue(let queue) = try await call(.queue(.init(offset: 0)))
  else { throw TestFailure.missingFixture }
  do {
    try await NativeAcceptance.checkControls(entry: entry, queue: queue, request: call)
    #expect(ignored == "success")
  } catch let failure as Failure {
    let hint = ["move": "순서", "remove": "삭제", "pause": "일시정지", "seek": "탐색",
                "mode-on": "모드", "mode-off": "모드", "resume": "재개", "stop": "정지"][ignored]
    #expect(hint != nil && failure.message.contains(hint!))
  }
}

private enum TestFailure: Error { case missingFixture }

@MainActor @Test(arguments: ["success", "unchanged", "wrong-index", "fail-once"])
func nativeTransitionsRequireFirstAttemptAndExactDuplicatePosition(_ broken: String) async throws {
  let service = try DemoService()
  defer { service.close() }
  var id: UInt64 = 0
  var plays = 0
  @MainActor func call(_ command: Command) async throws -> Notice {
    var command = command
    if case .play(var params) = command {
      plays += 1
      if broken == "unchanged" { return .ack(.init(message: "ignored")) }
      if broken == "fail-once", plays == 1 { throw TestFailure.missingFixture }
      if broken == "wrong-index", params.startIndex == 2 {
        params.startIndex = 1
        command = .play(params)
      }
    }
    id += 1
    let event: Event = await withCheckedContinuation { continuation in
      service.submit(try! Wire.encode(Request(version: 1, id: id, command: command))) {
        continuation.resume(returning: $0)
      }
    }
    if case .failure(let failure) = event.event { throw failure }
    return event.event
  }
  guard case .page(let songs) = try await call(.browse(.init(scope: .library, kind: .song, order: .name, offset: 0)))
  else { throw TestFailure.missingFixture }
  do {
    try await NativeAcceptance.checkTransitions(songs.items, request: call)
    #expect(broken == "success" && plays == 4)
  } catch { #expect(broken != "success" && plays == 1) }
}

@MainActor @Test(arguments: ["success", "home", "library", "detail"])
func nativeReadAcceptanceChecksHomeAndNeverSendsPlayback(_ broken: String) async throws {
  let service = try DemoService()
  defer { service.close() }
  var id: UInt64 = 0
  var scopes: Set<Scope> = []
  var kinds: Set<Kind> = []
  @MainActor func call(_ command: Command) async throws -> Notice {
    let stage: String
    switch command {
    case .search(let params): kinds.insert(params.kind); stage = "search"
    case .browse(let params): scopes.insert(params.scope); stage = params.scope.rawValue
    case .detail: stage = "detail"
    default: Issue.record("read check attempted a non-read command"); throw TestFailure.missingFixture
    }
    if stage == broken { return .ack(.init(message: "wrong result")) }
    id += 1
    let response: Event = await withCheckedContinuation { continuation in
      service.submit(try! Wire.encode(Request(version: 1, id: id, command: command))) {
        continuation.resume(returning: $0)
      }
    }
    if case .failure(let error) = response.event { throw error }
    return response.event
  }
  do {
    _ = try await NativeAcceptance.checkReads(query: "Mira", request: call)
    #expect(broken == "success")
    #expect(kinds == [.song, .album, .artist, .playlist, .station])
    #expect(scopes == [.home, .library])
  } catch {
    #expect(broken != "success")
  }
}
