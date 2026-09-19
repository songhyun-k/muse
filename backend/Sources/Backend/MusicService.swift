import Combine
import Foundation
import MusicContract
@preconcurrency import MusicKit

enum MusicEntity {
  case song(Song)
  case album(Album)
  case artist(Artist)
  case playlist(Playlist)
  case station(Station)

  func item(source: Source) -> Item {
    switch self {
    case .song(let song):
      return .init(
        ref: .init(id: song.id.rawValue, source: source, kind: .song),
        title: song.title, artist: song.artistName, album: song.albumTitle ?? "",
        duration: song.duration,
        artworkUrl: song.artwork?.url(width: 384, height: 384)?.absoluteString,
        releaseDate: song.releaseDate?.formatted(.iso8601),
        albumRef: song.albums?.first.map {
          .init(id: $0.id.rawValue, source: source, kind: .album)
        },
        artistRef: song.artists?.first.map {
          .init(id: $0.id.rawValue, source: source, kind: .artist)
        })
    case .album(let album):
      return .init(
        ref: .init(id: album.id.rawValue, source: source, kind: .album),
        title: album.title, artist: album.artistName, album: album.title,
        artworkUrl: album.artwork?.url(width: 384, height: 384)?.absoluteString,
        releaseDate: album.releaseDate?.formatted(.iso8601))
    case .artist(let artist):
      return .init(
        ref: .init(id: artist.id.rawValue, source: source, kind: .artist),
        title: artist.name, artist: artist.name, album: "",
        artworkUrl: artist.artwork?.url(width: 384, height: 384)?.absoluteString)
    case .playlist(let playlist):
      return .init(
        ref: .init(id: playlist.id.rawValue, source: source, kind: .playlist),
        title: playlist.name, artist: playlist.curatorName ?? "", album: "",
        artworkUrl: playlist.artwork?.url(width: 384, height: 384)?.absoluteString)
    case .station(let station):
      return .init(
        ref: .init(id: station.id.rawValue, source: source, kind: .station),
        title: station.name, artist: "", album: "",
        artworkUrl: station.artwork?.url(width: 384, height: 384)?.absoluteString)
    }
  }
}

@MainActor
final class MusicService {
  let web: WebMusic
  private let authorizationStatus: () -> MusicAuthorization.Status
  var entities: [String: MusicEntity] = [:]
  var player: ApplicationMusicPlayer?
  var playbackObservers: [AnyCancellable] = []
  var positionTask: Task<Void, Never>?
  var entryMetadata: [String: Item] = [:]
  var queueFingerprint: [String] = []
  var queueRevision: UInt64 = 0
  var radioPlaying = false
  var catalogPlayable: Bool?
  var onPlayback: ((PlayerState) -> Void)?
  var onFailure: ((Failure) -> Void)?

  init(web: WebMusic = WebMusic(), authorization: @escaping () -> MusicAuthorization.Status = {
    MusicAuthorization.currentStatus
  }) {
    self.web = web
    authorizationStatus = authorization
  }

  static func authorization(_ status: MusicAuthorization.Status) -> Authorization {
    switch status {
    case .authorized: .authorized
    case .denied: .denied
    case .restricted: .restricted
    case .notDetermined: .notDetermined
    @unknown default: .restricted
    }
  }

  func session() -> SessionState {
    let authorization = Self.authorization(authorizationStatus())
    return .init(
      authorization: authorization,
      canPlayCatalog: authorization == .authorized ? catalogPlayable : nil)
  }

  func authorize() async -> SessionState {
    let status = await MusicAuthorization.request()
    return await finishAuthorization(status) {
      try await MusicSubscription.current.canPlayCatalogContent
    }
  }

  func finishAuthorization(
    _ status: MusicAuthorization.Status, subscription: @MainActor () async throws -> Bool
  ) async -> SessionState {
    catalogPlayable = nil
    if status == .authorized {
      do { catalogPlayable = try await subscription() }
      catch { onFailure?(RequestScheduler.failure(error)) }
    }
    return .init(authorization: Self.authorization(status), canPlayCatalog: catalogPlayable)
  }

  func requireAuthorization() throws {
    guard authorizationStatus() == .authorized else {
      throw Failure(code: .notAuthorized, message: "음악 접근을 허용해주세요", retryable: false)
    }
  }

  func remember(_ entity: MusicEntity, source: Source) -> Item {
    let item = entity.item(source: source)
    if entities.count >= 4096 && entities[item.ref.storageKey] == nil {
      // ponytail: bounded cache reset; replace with LRU if repeated resolution is measured.
      entities.removeAll(keepingCapacity: true)
    }
    entities[item.ref.storageKey] = entity
    return item
  }

  func page<T: MusicItem>(
    _ collection: MusicItemCollection<T>, source: Source,
    offset: UInt64, entity: (T) -> MusicEntity
  ) -> Page {
    .init(
      items: collection.map { remember(entity($0), source: source) },
      nextOffset: !collection.isEmpty && collection.hasNextBatch
        ? offset + UInt64(collection.count) : nil)
  }

  func search(_ params: SearchParams) async throws -> Page {
    try requireAuthorization()
    if params.source == .library {
      return try await library(
        .init(scope: .library, kind: params.kind, order: .name, offset: params.offset),
        query: params.query)
    }
    guard params.source == .catalog else {
      throw Failure(code: .unavailable, message: "이 범위에서는 검색할 수 없습니다", retryable: false)
    }
    switch params.kind {
    case .song: return try await catalogSearch(params, entity: MusicEntity.song)
    case .album: return try await catalogSearch(params, entity: MusicEntity.album)
    case .artist: return try await catalogSearch(params, entity: MusicEntity.artist)
    case .playlist: return try await catalogSearch(params, entity: MusicEntity.playlist)
    case .station: return try await catalogSearch(params, entity: MusicEntity.station)
    }
  }

  private func catalogSearch<T: Decodable>(
    _ params: SearchParams, entity: (T) -> MusicEntity
  ) async throws -> Page {
    let page: WebPage<T> = try await web.search(params)
    try Task.checkCancellation()
    return .init(items: page.data.map { remember(entity($0), source: .catalog) },
                 nextOffset: try page.nextOffset(after: params.offset))
  }
}
