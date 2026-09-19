import Backend
import Foundation
import MusicContract

/// An explicit diagnostic client of the public service contract, never a UI backend.
@MainActor
enum NativeAcceptance {
  static func run(_ arguments: [String]) async -> Int32 {
    let readOnly = arguments.sorted() == ["--live-check", "--read-only"]
    guard arguments == ["--live-check"] || readOnly else {
      fputs("검사는 --live-check 또는 --live-check --read-only로 실행해주세요.\n", stderr)
      return 2
    }
    let service = Service()  // Ephemeral store: the check cannot change saved collections/history.
    let timeout = Task {
      do { try await Task.sleep(for: .seconds(60)) } catch { return }
      service.close()
      fputs("MusicKit 검사 시간이 초과되었습니다.\n", stderr)
      exit(2)
    }
    defer {
      timeout.cancel()
      service.close()
    }
    var id: UInt64 = 0
    @MainActor func request(_ command: Command) async throws -> Notice {
      id += 1
      let event = await service.handle(
        try Wire.encode(Request(version: apiVersion, id: id, command: command)))
      guard event.id == id else { throw unavailable("응답 식별자가 일치하지 않습니다") }
      if case .failure(let failure) = event.event { throw failure }
      return event.event
    }
    do {
      print("MusicKit: 권한 확인")
      guard case .session(let session) = try await request(.authorize(.init())),
        session.authorization == .authorized
      else { throw unavailable("음악 접근 권한을 허용해주세요") }
      let songs = try await checkReads(request: request)
      if readOnly {
        print("MusicKit READ PASS: 검색·홈·상세·보관함, 재생 조작 없음")
        return 0
      }
      guard session.canPlayCatalog == true else { throw unavailable("Apple Music 구독 상태를 확인해주세요") }
      print("MusicKit: 조회 확인, 곡 전환 검사")
      try await checkTransitions(songs, request: request)
      let song = songs[0]
      _ = try await request(
        .play(
          .init(
            items: [song.ref, song.ref, song.ref], startIndex: 0, placement: .replace,
            shuffle: false)))
      let started = try await waitForPlayer("재생이 시작되지 않았습니다", request: request) {
        $0.playing && $0.current?.ref == song.ref && $0.currentEntryId != nil
      }.player
      let entry = started.currentEntryId!
      try await Task.sleep(for: .seconds(2))
      let snapshot = try await waitForPlayer("재생 시간이 진행되지 않았습니다", request: request) {
        $0.playing && $0.currentEntryId == entry && $0.position > started.position + 0.2
      }
      guard
        case .queue(let queue) = try await request(
          .queue(.init(offset: 0, revision: snapshot.player.queueRevision))),
        queue.entries.count == 2, queue.entries[0].id != queue.entries[1].id
      else { throw unavailable("재생 큐의 곡 식별자를 확인할 수 없습니다") }
      try await checkControls(entry: entry, queue: queue, request: request)
      guard case .lyrics(let lyrics) = try await request(.lyrics(.init(item: song.ref)))
      else { throw unavailable("가사 응답을 확인할 수 없습니다") }
      print("MusicKit PASS: 권한·검색·홈·상세·보관함·곡 전환·재생·큐·탐색·모드·정지")
      print("가사: \(lyrics.status.rawValue), 장치 음량 변경 지원: \(snapshot.volume.canSetVolume)")
      return 0
    } catch {
      if !readOnly { _ = try? await request(.control(.init(action: .stop))) }
      let message = (error as? Failure)?.message ?? "MusicKit 검사를 완료하지 못했습니다"
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
      else { throw unavailable("검색 응답을 확인할 수 없습니다") }
      print("검색 \(kind.rawValue): \(page.items.count)개")
      if kind == .song { playable = page.items.filter { ($0.duration ?? 0) > 10 } }
      if let offset = page.nextOffset {
        guard case .page = try await request(
          .search(.init(query: query, source: .catalog, kind: kind, offset: offset)))
        else { throw unavailable("검색 다음 페이지를 확인할 수 없습니다") }
      }
      if let item = page.items.first, kind != .station {
        guard case .detail(let detail) = try await request(.detail(.init(item: item.ref, offset: 0)))
        else { throw unavailable("상세 응답을 확인할 수 없습니다") }
        if let offset = detail.nextOffset {
          guard case .detail = try await request(.detail(.init(item: detail.item.ref, offset: offset)))
          else { throw unavailable("상세 다음 페이지를 확인할 수 없습니다") }
        }
      }
    }
    guard case .page(let home) = try await request(.browse(.init(scope: .home, kind: .album, order: .recent, offset: 0))),
      case .page(let library) = try await request(.browse(.init(scope: .library, kind: .song, order: .recent, offset: 0)))
    else { throw unavailable("홈·보관함 응답을 확인할 수 없습니다") }
    print("홈: \(home.items.count)개, 보관함: \(library.items.count)개")
    guard !playable.isEmpty else { throw unavailable("재생할 검색 결과가 없습니다") }
    return playable
  }

  static func checkTransitions(
    _ songs: [Item], request: @MainActor (Command) async throws -> Notice
  ) async throws {
    let refs = songs.prefix(3).map(\.ref)
    guard Set(refs.map(\.id)).count == 3 else { throw unavailable("전환을 검사할 서로 다른 곡이 부족합니다") }
    let steps: [([ItemRef], UInt64, UInt64)] = [
      ([refs[0], refs[1], refs[1], refs[2]], 2, 1),
      ([refs[0]], 0, 0), ([refs[2]], 0, 0), ([refs[1]], 0, 0)
    ]
    var previous: String?
    for (items, start, upcoming) in steps {
      _ = try await request(.play(.init(items: items, startIndex: start, placement: .replace, shuffle: false)))
      let state = try await waitForPlayer("곡 전환의 시작 위치와 재생 상태가 일치하지 않습니다", request: request) {
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
        throw unavailable("상태 응답을 확인할 수 없습니다")
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
        throw unavailable("큐 응답을 확인할 수 없습니다")
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
      throw unavailable("재생 큐의 곡 식별자를 확인할 수 없습니다")
    }
    let ids = queue.entries.map(\.id)
    _ = try await request(.queueMove(.init(entryId: ids[1], beforeEntryId: ids[0])))
    try await waitForQueue(ids.reversed(), message: "큐 순서 변경이 확인되지 않았습니다", request: request)
    _ = try await request(.queueRemove(.init(entryId: ids[0])))
    try await waitForQueue([ids[1]], message: "큐 삭제가 확인되지 않았습니다", request: request)
    _ = try await request(.control(.init(action: .pause)))
    _ = try await waitForPlayer("일시정지가 확인되지 않았습니다", request: request) {
      !$0.playing && $0.currentEntryId == entry
    }
    _ = try await request(.seek(.init(seconds: 1, entryId: entry)))
    _ = try await waitForPlayer("탐색이 확인되지 않았습니다", request: request) {
      $0.currentEntryId == entry && abs($0.position - 1) < 0.25
    }
    for enabled in [true, false] {
      let mode: RepeatMode = enabled ? .all : .off
      _ = try await request(.mode(.init(shuffle: enabled, repeatMode: mode)))
      _ = try await waitForPlayer("재생 모드 변경이 확인되지 않았습니다", request: request) {
        $0.shuffle == enabled && $0.repeatMode == mode
      }
    }
    _ = try await request(.control(.init(action: .toggle)))
    _ = try await waitForPlayer("재생 재개가 확인되지 않았습니다", request: request) { $0.playing }
    _ = try await request(.control(.init(action: .stop)))
    _ = try await waitForPlayer("재생 정지가 확인되지 않았습니다", request: request) { !$0.playing }
  }

  private static func unavailable(_ message: String) -> Failure {
    .init(code: .unavailable, message: message, retryable: true)
  }
}
