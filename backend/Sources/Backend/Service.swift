import AppKit
import Foundation
import MusicContract

@MainActor
public final class Service {
  public var onEvent: (@MainActor (Event) -> Void)?
  let store: Result<LibraryStore, Error>
  let music: MusicService
  private let artwork = ArtworkService()
  let lyrics: LyricsService
  let volume = VolumeService()
  private var historyEntry: String?
  private var historyReference: ItemRef?
  private var loginTask: Task<Void, Never>?
  private var loginPresented = false
  private let openMusic: @MainActor () async throws -> Void
  private lazy var scheduler = RequestScheduler { [weak self] request in
    guard let self else { throw CancellationError() }
    do {
      let result = try await self.perform(request)
      switch request.command {
      case .search(let p) where p.source == .catalog: self.loginPresented = false
      case .browse(let p) where p.scope == .home: self.loginPresented = false
      default: break
      }
      return result
    } catch { throw await self.loginFailure(error) }
  }

  public convenience init(storeURL: URL? = nil, lyricsSession: URLSession = .shared) {
    self.init(storeURL: storeURL, lyricsSession: lyricsSession, music: MusicService())
  }

  init(storeURL: URL? = nil, lyricsSession: URLSession = .shared, music: MusicService,
       openMusic: @escaping @MainActor () async throws -> Void = {
         guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Music")
         else { throw CocoaError(.fileNoSuchFile) }
         _ = try await NSWorkspace.shared.openApplication(at: url, configuration: .init())
       }) {
    self.music = music
    self.openMusic = openMusic
    lyrics = LyricsService(session: lyricsSession)
    store = Result {
      do { return try LibraryStore(file: storeURL) } catch let error as StoreError {
        throw error
      } catch { throw StoreError.read }
    }
    music.onPlayback = { [weak self] state in self?.playbackChanged(state) }
    music.onFailure = { [weak self] failure in self?.publish(.failure(failure)) }
    volume.onChange = { [weak self] state in self?.publish(.volume(state)) }
  }

  public func handle(_ data: Data) async -> Event {
    await withCheckedContinuation { continuation in
      submit(data) { continuation.resume(returning: $0) }
    }
  }

  public func submit(_ data: Data, reply: @escaping @MainActor (Event) -> Void) {
    scheduler.submit(data, reply: reply)
  }

  public func close() {
    loginTask?.cancel()
    scheduler.close()
    music.closePlayer()
    music.web.close()
    volume.close()
  }

  public func start() {
    volume.start()
    loginTask = Task {
      do { try await music.web.checkLogin() }
      catch where RequestScheduler.failure(error).code == .signInRequired {
        let failure = await loginFailure(error)
        if !Task.isCancelled { publish(.failure(failure)) }
      } catch { /* Background connectivity does not block the library. */ }
    }
  }

  private func loginFailure(_ error: Error) async -> Failure {
    let failure = RequestScheduler.failure(error)
    guard failure.code == .signInRequired, !loginPresented, !Task.isCancelled else { return failure }
    loginPresented = true
    do { try await openMusic() }
    catch {
      return .init(code: .signInRequired, message: "음악 앱을 직접 열어 로그인해주세요", retryable: true)
    }
    return failure
  }

  private func publish(_ notice: Notice) {
    if let onEvent { scheduler.notify(notice, reply: onEvent) }
  }

  func playbackChanged(_ state: PlayerState) {
    publish(.player(state))
    guard state.playing, let item = state.current, item.ref.kind == .song,
      state.currentEntryId != historyEntry || item.ref != historyReference
    else { return }
    historyEntry = state.currentEntryId
    historyReference = item.ref
    do {
      let library = try store.get()
      try library.recordPlayback(item)
      publish(.store(library.summary))
    } catch {
      publish(.failure(.init(code: .storage, message: "재생 이력을 저장하지 못했습니다", retryable: true)))
    }
  }

  private func perform(_ request: Request) async throws -> Notice {
    switch request.command {
    case .snapshot:
      return .snapshot(
        .init(
          session: music.session(), player: music.playbackState(),
          store: try store.get().summary,
          volume: volume.state()))
    case .authorize: return .session(await music.authorize())
    case .play(let params): return .player(try await play(params))
    case .control(let params): return .player(try await music.control(params.action))
    case .seek(let params): return .player(try music.seek(params))
    case .mode(let params): return .player(try music.mode(params))
    case .queue(let params): return .queue(try music.queuePage(params))
    case .queueRemove(let params): return .player(try music.removeQueueEntry(params.entryId))
    case .queueMove(let params): return .player(try music.moveQueueEntry(params))
    case .queueJump(let params): return .player(try await music.jumpQueueEntry(params.entryId))
    case .queueClear: return .player(music.clearQueue())
    case .volume(let params): return .volume(try volume.set(params))
    case .lyrics(let params):
      let item = try await metadata(params.item)
      let preference = try store.get().state.lyrics[params.item.storageKey] ?? .init()
      return .lyrics(try await lyrics.load(item, preference: preference))
    case .lyricsSearch(let params):
      return .lyricsMatches(try await lyrics.search(params.query, for: params.item))
    case .lyricsChoose(let params):
      guard let record = try await lyrics.record(params.matchId), record.id == params.matchId else {
        throw Failure(code: .notFound, message: "선택한 가사를 찾을 수 없습니다", retryable: true)
      }
      let library = try store.get()
      let preference = LyricPreference(
        matchId: record.id, offset: library.state.lyrics[params.item.storageKey]?.offset ?? 0)
      let value = try lyrics.remember(record, for: params.item, offset: preference.offset)
      try library.lyricPreference(preference, for: params.item)
      return .lyrics(value)
    case .lyricsOffset(let params):
      let library = try store.get()
      var preference = library.state.lyrics[params.item.storageKey] ?? .init()
      var value = try await lyrics.load(try await metadata(params.item), preference: preference)
      guard value.status == .synced else {
        throw Failure(code: .unavailable, message: "동기화 가사에서 시간을 조절할 수 있습니다", retryable: false)
      }
      preference.offset = params.seconds
      try library.lyricPreference(preference, for: params.item)
      value.offset = params.seconds
      return .lyrics(value)
    case .search(let params): return .page(try await music.search(params))
    case .detail(let params):
      if params.item.source == .collection { return .detail(try localDetail(params)) }
      return .detail(try await music.detail(params))
    case .artwork(let params):
      if params.item.source == .collection {
        guard
          var item = try store.get().state.collections.first(where: { $0.id == params.item.id })?
            .items.first
        else {
          throw Failure(code: .notFound, message: "앨범 이미지가 없습니다", retryable: false)
        }
        item.ref = params.item
        return .artwork(try await artwork.get(item))
      }
      let item = try await music.resolve(params.item).item(source: params.item.source)
      return .artwork(try await artwork.get(item))
    case .collectionCreate(let p):
      var items: [Item] = []
      for reference in p.items ?? [] { items.append(try await metadata(reference)) }
      return .collectionCreated(
        try store.get().create(name: p.name, description: p.description, items: items))
    case .collectionUpdate(let p): return .store(try store.get().update(p))
    case .collectionDelete(let p): return .store(try store.get().delete(p.id))
    case .collectionRemove(let p): return .store(try store.get().remove(p))
    case .collectionMove(let p): return .store(try store.get().move(p))
    case .collectionCopy(let p):
      let items: [Item]
      if p.item.source == .collection {
        guard
          let collection = try store.get().state.collections.first(where: { $0.id == p.item.id })
        else {
          throw Failure(code: .notFound, message: "플레이리스트를 찾을 수 없습니다", retryable: false)
        }
        items = collection.items
      } else {
        items = try await music.songs(p.item)
      }
      return .collectionCreated(try store.get().create(name: p.name, description: "", items: items))
    case .collectionAdd(let p):
      var items: [Item] = []
      for reference in p.items { items.append(try await metadata(reference)) }
      return .store(try store.get().add(items, to: p.id))
    case .favorite(let p):
      let library = try store.get()
      if !p.enabled && library.savedItem(p.item) == nil { return .store(library.summary) }
      return .store(try library.favorite(try await metadata(p.item), enabled: p.enabled))
    case .browse(let p) where p.scope == .library || p.scope == .recent:
      return .page(try await music.library(p))
    case .browse(let p) where p.scope == .home:
      return .page(try await music.recommendations(offset: p.offset))
    case .browse(let p):
      if let page = try store.get().localPage(p) { return .page(page) }
    case .cancel:
      throw Failure(code: .invalidRequest, message: "취소 요청 형식을 확인해주세요", retryable: false)
    }
    return .failure(.init(code: .unavailable, message: "지원하지 않는 요청입니다", retryable: false))
  }

  func metadata(_ reference: ItemRef) async throws -> Item {
    if let saved = try store.get().savedItem(reference) { return saved }
    return try await music.resolve(reference).item(source: reference.source)
  }

  private func play(_ params: PlayParams) async throws -> PlayerState {
    var references: [ItemRef] = []
    var start = 0
    for (index, reference) in params.items.enumerated() {
      if index == Int(params.startIndex) { start = references.count }
      if reference.source == .collection {
        guard
          let collection = try store.get().state.collections.first(where: { $0.id == reference.id })
        else {
          throw Failure(code: .notFound, message: "플레이리스트를 찾을 수 없습니다", retryable: false)
        }
        references += collection.items.map(\.ref)
      } else if reference.kind == .song || reference.kind == .station {
        references.append(reference)
      } else {
        references += try await music.songs(reference).map(\.ref)
      }
      guard references.count <= 2000 else {
        throw Failure(code: .conflict, message: "한 번에 2000곡까지 재생할 수 있습니다", retryable: false)
      }
    }
    return try await music.play(
      references, startIndex: start, placement: params.placement, shuffle: params.shuffle)
  }

  private func localDetail(_ params: DetailParams) throws -> Detail {
    let library = try store.get()
    guard let collection = library.state.collections.first(where: { $0.id == params.item.id }),
      let page = try library.localPage(
        .init(
          scope: .collection, kind: .song,
          collectionId: params.item.id, order: .recent, offset: params.offset))
    else {
      throw Failure(code: .notFound, message: "플레이리스트를 찾을 수 없습니다", retryable: false)
    }
    let item = Item(
      ref: params.item, title: collection.name, artist: "", album: "",
      duration: collection.items.compactMap(\.duration).reduce(0, +),
      artworkUrl: collection.items.first?.artworkUrl)
    return .init(item: item, children: page.items, nextOffset: page.nextOffset)
  }
}
