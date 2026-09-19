import Foundation
import MusicContract

@MainActor
public final class DemoService {
  public var onEvent: (@MainActor (Event) -> Void)?
  let catalog: DemoCatalog
  let store: LibraryStore
  var player: PlayerState
  var volume: VolumeState
  var queue: [QueueEntry]
  var past: [Item] = []
  private var timer: Task<Void, Never>?
  private lazy var scheduler = RequestScheduler { [weak self] request in
    guard let self else { throw CancellationError() }
    self.advance(to: Date().timeIntervalSince1970)
    return try self.perform(request.command)
  }

  public init() throws {
    catalog = try DemoCatalog.load()
    store = try LibraryStore()
    player = catalog.player
    player.updatedAt = Date().timeIntervalSince1970
    volume = catalog.volume
    queue = catalog.queue
    try store.transaction { $0 = catalog.seed }
  }

  public func submit(_ data: Data, reply: @escaping @MainActor (Event) -> Void) {
    scheduler.submit(data, reply: reply)
  }

  public func start() {
    guard timer == nil else { return }
    timer = Task { [weak self] in
      while !Task.isCancelled {
        do { try await Task.sleep(for: .milliseconds(500)) } catch { return }
        guard let self else { return }
        if self.player.playing {
          self.advance(to: Date().timeIntervalSince1970)
          self.publish(.player(self.player))
        }
      }
    }
  }

  public func close() {
    timer?.cancel()
    timer = nil
    scheduler.close()
  }

  func publish(_ notice: Notice) {
    if let onEvent { scheduler.notify(notice, reply: onEvent) }
  }

  private func perform(_ command: Command) throws -> Notice {
    switch command {
    case .snapshot:
      return .snapshot(
        .init(
          session: .init(authorization: .authorized, canPlayCatalog: true),
          player: player, store: store.summary, volume: volume))
    case .authorize: return .session(.init(authorization: .authorized, canPlayCatalog: true))
    case .search(let p): return .page(try catalog.search(p))
    case .browse(let p): return .page(try store.localPage(p) ?? catalog.browse(p))
    case .detail(let p):
      if p.item.source != .collection { return .detail(try catalog.detail(p)) }
      let saved = try collection(p.item.id)
      let page = catalog.page(saved.items, offset: p.offset)
      return .detail(
        .init(
          item: .init(ref: p.item, title: saved.name, artist: "플레이리스트", album: ""),
          children: page.items, nextOffset: page.nextOffset))
    case .artwork(let p): return .artwork(try catalog.artwork(p.item))
    case .collectionCreate(let p):
      return .collectionCreated(
        try store.create(
          name: p.name, description: p.description,
          items: (p.items ?? []).map(catalog.item)))
    case .collectionUpdate(let p): return .store(try store.update(p))
    case .collectionDelete(let p): return .store(try store.delete(p.id))
    case .collectionAdd(let p): return .store(try store.add(p.items.map(catalog.item), to: p.id))
    case .collectionRemove(let p): return .store(try store.remove(p))
    case .collectionMove(let p): return .store(try store.move(p))
    case .collectionCopy(let p):
      return .collectionCreated(
        try store.create(name: p.name, description: "", items: songs(p.item)))
    case .favorite(let p):
      return .store(try store.favorite(catalog.item(p.item), enabled: p.enabled))
    case .lyrics(let p): return .lyrics(try lyric(p.item))
    case .lyricsSearch(let p):
      let words = p.query.split(whereSeparator: \.isWhitespace)
      let matches = catalog.tracks.enumerated().filter { _, song in
        words.allSatisfy { "\(song.artist) \(song.title)".localizedStandardContains(String($0)) }
      }.map { index, song in
        LyricMatch(
          id: UInt64(index + 1), title: song.title, artist: song.artist, album: song.album,
          duration: song.duration, synced: true)
      }
      return .lyricsMatches(.init(item: p.item, matches: matches))
    case .lyricsChoose(let p):
      guard p.matchId > 0, p.matchId <= catalog.tracks.count else { throw DemoCatalog.missing() }
      var preference = store.state.lyrics[p.item.storageKey] ?? .init()
      preference.matchId = p.matchId
      try store.lyricPreference(preference, for: p.item)
      return .lyrics(try lyric(p.item))
    case .lyricsOffset(let p):
      _ = try catalog.item(p.item)
      var preference = store.state.lyrics[p.item.storageKey] ?? .init()
      preference.offset = p.seconds
      try store.lyricPreference(preference, for: p.item)
      return .lyrics(try lyric(p.item))
    case .volume(let p):
      volume.level = p.level ?? volume.level
      volume.muted = p.muted ?? volume.muted
      return .volume(volume)
    case .play(let p): try play(p)
    case .control(let p): try control(p.action)
    case .seek(let p):
      guard p.entryId == player.currentEntryId else { throw conflict() }
      player.position = min(p.seconds, player.current?.duration ?? 0)
    case .mode(let p):
      player.shuffle = p.shuffle
      player.repeatMode = p.repeatMode
    case .queue(let p):
      guard p.revision == nil || p.revision == player.queueRevision else { throw conflict() }
      let start = Int(min(p.offset, UInt64(queue.count)))
      let end = min(start + 50, queue.count)
      return .queue(
        .init(
          entries: Array(queue[start..<end]), nextOffset: end < queue.count ? UInt64(end) : nil,
          revision: player.queueRevision, total: UInt64(queue.count)))
    case .queueRemove(let p):
      queue.remove(at: try queueIndex(p.entryId))
      revised()
    case .queueMove(let p):
      let index = try queueIndex(p.entryId)
      if let id = p.beforeEntryId { _ = try queueIndex(id) }
      if p.beforeEntryId != p.entryId {
        let entry = queue.remove(at: index)
        queue.insert(
          entry,
          at: p.beforeEntryId.flatMap { id in queue.firstIndex { $0.id == id } } ?? queue.count)
        revised()
      }
    case .queueJump(let p):
      let index = try queueIndex(p.entryId)
      let entry = queue[index]
      queue.removeFirst(index + 1)
      try enter(entry)
    case .queueClear:
      queue.removeAll()
      revised()
    case .cancel: throw conflict()  // RequestScheduler consumes cancellation before dispatch.
    }
    return .player(player)
  }

  func collection(_ id: String) throws -> SavedCollection {
    guard let collection = store.state.collections.first(where: { $0.id == id }) else {
      throw DemoCatalog.missing()
    }
    return collection
  }

  func songs(_ reference: ItemRef) throws -> [Item] {
    try reference.source == .collection ? collection(reference.id).items : catalog.songs(reference)
  }

  private func lyric(_ reference: ItemRef) throws -> Lyrics {
    let preference = store.state.lyrics[reference.storageKey] ?? .init()
    let selected = preference.matchId.map { catalog.tracks[Int($0 - 1)].ref } ?? reference
    var result = try catalog.lyric(selected, preference: preference)
    result.item = reference
    return result
  }

  func conflict() -> Failure {
    .init(code: .conflict, message: "목록이 변경되었습니다. 다시 선택해주세요", retryable: true)
  }
}
