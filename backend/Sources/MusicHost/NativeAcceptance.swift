import Backend
import Foundation
import MusicContract

/// An explicit diagnostic client of the public service contract, never a UI backend.
@MainActor
enum NativeAcceptance {
  static func run(_ arguments: [String]) async -> Int32 {
    let readOnly = arguments.sorted() == ["--live-check", "--read-only"]
    guard arguments == ["--live-check"] || readOnly else {
      fputs("Usage: muse --live-check [--read-only]\n", stderr)
      return 2
    }
    let service = Service()  // Ephemeral store: the check cannot change saved collections/history.
    let timeout = Task {
      do { try await Task.sleep(for: .seconds(60)) } catch { return }
      service.close()
      fputs("MusicKit check timed out.\n", stderr)
      exit(2)
    }
    defer {
      timeout.cancel()
      service.close()
    }
    var id: UInt64 = 0
    @MainActor @Sendable func request(_ command: Command) async throws -> Notice {
      id += 1
      let event = await service.handle(
        try Wire.encode(Request(version: apiVersion, id: id, command: command)))
      guard event.id == id else { throw unavailable("Response ID does not match the request") }
      if case .failure(let failure) = event.event { throw failure }
      return event.event
    }
    do {
      print("MusicKit: checking authorization")
      guard case .session(let session) = try await request(.authorize(.init())),
        session.authorization == .authorized
      else { throw unavailable("Allow access to your music") }
      let songs = try await checkReads(request: request)
      if readOnly {
        print("MusicKit READ PASS: search, Home, details, library; no playback changes")
        return 0
      }
      guard session.canPlayCatalog == true else { throw unavailable("Check your Apple Music subscription") }
      print("MusicKit: reads verified; checking song transitions")
      try await checkTransitions(songs, request: request)
      let song = songs[0]
      _ = try await request(
        .play(
          .init(
            items: [song.ref, song.ref, song.ref], startIndex: 0, placement: .replace,
            shuffle: false)))
      let started = try await waitForPlayer("Playback did not start", request: request) {
        $0.playing && $0.current?.ref == song.ref && $0.currentEntryId != nil
      }.player
      let entry = started.currentEntryId!
      try await Task.sleep(for: .seconds(2))
      let snapshot = try await waitForPlayer("Playback time did not advance", request: request) {
        $0.playing && $0.currentEntryId == entry && $0.position > started.position + 0.2
      }
      guard
        case .queue(let queue) = try await request(
          .queue(.init(offset: 0, revision: snapshot.player.queueRevision))),
        queue.entries.count == 2, queue.entries[0].id != queue.entries[1].id
      else { throw unavailable("Could not verify unique queue entry IDs") }
      try await checkControls(entry: entry, queue: queue, request: request)
      guard case .lyrics(let lyrics) = try await request(.lyrics(.init(item: song.ref)))
      else { throw unavailable("Could not verify the lyrics response") }
      print("MusicKit PASS: authorization, search, Home, details, library, song transitions, playback, queue, seek, modes, stop")
      print("Lyrics: \(lyrics.status.rawValue), device volume control supported: \(snapshot.volume.canSetVolume)")
      return 0
    } catch {
      if !readOnly { _ = try? await request(.control(.init(action: .stop))) }
      let message = (error as? Failure)?.message ?? "Could not complete the MusicKit check"
      let failure = error as? Failure
      fputs("MusicKit FAIL [\(failure?.code.rawValue ?? "internal_error"), retryable=\(failure?.retryable ?? false)]: \(message)\n", stderr)
      return 1
    }
  }

  static func checkReads(query: String = "Ado", request: @MainActor (Command) async throws -> Notice) async throws -> [Item] {
    var playable: [Item] = []
    for kind in [Kind.song, .album, .artist, .playlist, .station] {
      guard case .page(let page) = try await request(
        .search(.init(query: query, source: .catalog, kind: kind, offset: 0)))
      else { throw unavailable("Could not verify the search response") }
      print("Search \(kind.rawValue): \(page.items.count)")
      if kind == .song { playable = page.items.filter { ($0.duration ?? 0) > 10 } }
      if let offset = page.nextOffset {
        guard case .page = try await request(
          .search(.init(query: query, source: .catalog, kind: kind, offset: offset)))
        else { throw unavailable("Could not verify the next search page") }
      }
      if let item = page.items.first, kind != .station {
        guard case .detail(let detail) = try await request(.detail(.init(item: item.ref, offset: 0)))
        else { throw unavailable("Could not verify the detail response") }
        if let offset = detail.nextOffset {
          guard case .detail = try await request(.detail(.init(item: detail.item.ref, offset: offset)))
          else { throw unavailable("Could not verify the next detail page") }
        }
      }
    }
    guard case .page(let home) = try await request(.browse(.init(scope: .home, kind: .album, order: .recent, offset: 0))),
      case .page(let library) = try await request(.browse(.init(scope: .library, kind: .song, order: .recent, offset: 0)))
    else { throw unavailable("Could not verify the Home and library responses") }
    print("Home: \(home.items.count), library: \(library.items.count)")
    guard !playable.isEmpty else { throw unavailable("No playable search results") }
    return playable
  }

  static func checkTransitions(
    _ songs: [Item], request: @MainActor (Command) async throws -> Notice
  ) async throws {
    let refs = songs.prefix(3).map(\.ref)
    guard Set(refs.map(\.id)).count == 3 else { throw unavailable("Not enough distinct songs to check transitions") }
    let steps: [([ItemRef], UInt64, UInt64)] = [
      ([refs[0], refs[1], refs[1], refs[2]], 2, 1),
      ([refs[0]], 0, 0), ([refs[2]], 0, 0), ([refs[1]], 0, 0)
    ]
    var previous: String?
    for (items, start, upcoming) in steps {
      _ = try await request(.play(.init(items: items, startIndex: start, placement: .replace, shuffle: false)))
      let state = try await waitForPlayer("Song transition did not reach the expected playback state", request: request) {
        $0.playing && $0.current?.ref == items[Int(start)] && $0.queueCount == upcoming
          && $0.currentEntryId != nil && $0.currentEntryId != previous
      }.player
      previous = state.currentEntryId
    }
  }

  static func waitForPlayer(
    _ message: String, request: @MainActor (Command) async throws -> Notice,
    ready: (PlayerState) -> Bool
  ) async throws -> Snapshot {
    for _ in 0..<20 {
      guard case .snapshot(let value) = try await request(.snapshot(.init())) else {
        throw unavailable("Could not verify the snapshot response")
      }
      if ready(value.player) { return value }
      try await Task.sleep(for: .milliseconds(100))
    }
    throw unavailable(message)
  }

  private static func waitForQueue(
    _ ids: [String], message: String, request: @MainActor (Command) async throws -> Notice
  ) async throws {
    for _ in 0..<20 {
      guard case .queue(let queue) = try await request(.queue(.init(offset: 0))) else {
        throw unavailable("Could not verify the queue response")
      }
      if queue.entries.map(\.id) == ids && queue.total == ids.count { return }
      try await Task.sleep(for: .milliseconds(100))
    }
    throw unavailable(message)
  }

  static func checkControls(
    entry: String, queue: QueuePage, request: @MainActor (Command) async throws -> Notice
  ) async throws {
    guard queue.entries.count == 2, queue.entries[0].id != queue.entries[1].id else {
      throw unavailable("Could not verify unique queue entry IDs")
    }
    let ids = queue.entries.map(\.id)
    _ = try await request(.queueMove(.init(entryId: ids[1], beforeEntryId: ids[0])))
    try await waitForQueue(ids.reversed(), message: "Queue order did not change", request: request)
    _ = try await request(.queueRemove(.init(entryId: ids[0])))
    try await waitForQueue([ids[1]], message: "Queue entry was not removed", request: request)
    _ = try await request(.control(.init(action: .pause)))
    _ = try await waitForPlayer("Playback did not pause", request: request) {
      !$0.playing && $0.currentEntryId == entry
    }
    _ = try await request(.seek(.init(seconds: 1, entryId: entry)))
    _ = try await waitForPlayer("Playback did not seek", request: request) {
      $0.currentEntryId == entry && abs($0.position - 1) < 0.25
    }
    for enabled in [true, false] {
      let mode: RepeatMode = enabled ? .all : .off
      _ = try await request(.mode(.init(shuffle: enabled, repeatMode: mode)))
      _ = try await waitForPlayer("Playback mode did not change", request: request) {
        $0.shuffle == enabled && $0.repeatMode == mode
      }
    }
    _ = try await request(.control(.init(action: .toggle)))
    _ = try await waitForPlayer("Playback did not resume", request: request) { $0.playing }
    _ = try await request(.control(.init(action: .stop)))
    _ = try await waitForPlayer("Playback did not stop", request: request) { !$0.playing }
  }

  private static func unavailable(_ message: String) -> Failure {
    .init(code: .unavailable, message: message, retryable: true)
  }
}
