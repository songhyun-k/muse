import Foundation
import MusicContract
@preconcurrency import MusicKit

extension MusicService {
  func library(_ params: BrowseParams, query: String? = nil) async throws -> Page {
    try requireAuthorization()
    let recent = params.scope == .recent || params.order == .recent
    switch params.kind {
    case .song:
      var request = MusicLibraryRequest<Song>()
      if recent {
        request.sort(by: \.libraryAddedDate, ascending: false)
      } else if params.order == .artist {
        request.sort(by: \.artistName, ascending: true)
      } else {
        request.sort(by: \.title, ascending: true)
      }
      return try await libraryPage(
        request, offset: params.offset, query: query, entity: MusicEntity.song)
    case .album:
      var request = MusicLibraryRequest<Album>()
      if recent {
        request.sort(by: \.libraryAddedDate, ascending: false)
      } else if params.order == .artist {
        request.sort(by: \.artistName, ascending: true)
      } else {
        request.sort(by: \.title, ascending: true)
      }
      return try await libraryPage(
        request, offset: params.offset, query: query, entity: MusicEntity.album)
    case .artist:
      var request = MusicLibraryRequest<Artist>()
      if recent {
        request.sort(by: \.libraryAddedDate, ascending: false)
      } else {
        request.sort(by: \.name, ascending: true)
      }
      return try await libraryPage(
        request, offset: params.offset, query: query, entity: MusicEntity.artist)
    case .playlist:
      var request = MusicLibraryRequest<Playlist>()
      if recent {
        request.sort(by: \.libraryAddedDate, ascending: false)
      } else {
        request.sort(by: \.name, ascending: true)
      }
      return try await libraryPage(
        request, offset: params.offset, query: query, entity: MusicEntity.playlist)
    case .station:
      throw Failure(code: .unavailable, message: "스테이션은 전체 검색에서 찾을 수 있습니다", retryable: false)
    }
  }

  private func libraryPage<T: MusicLibraryRequestable>(
    _ initial: MusicLibraryRequest<T>, offset: UInt64,
    query: String?, entity: (T) -> MusicEntity
  ) async throws -> Page {
    var request = initial
    request.limit = 50
    request.offset = Int(offset)
    if let query { request.filter(text: query.trimmingCharacters(in: .whitespacesAndNewlines)) }
    let response = try await request.response()
    try Task.checkCancellation()
    return page(response.items, source: .library, offset: offset, entity: entity)
  }

  func resolve(_ reference: ItemRef) async throws -> MusicEntity {
    if let cached = entities[reference.storageKey] { return cached }
    try requireAuthorization()
    guard reference.source != .collection else {
      throw Failure(code: .unavailable, message: "플레이리스트에서 노래를 선택해주세요", retryable: false)
    }
    let entity: MusicEntity
    switch reference.kind {
    case .song:
      entity = try await lookup(
        reference, libraryKey: \.id, entity: MusicEntity.song)
    case .album:
      entity = try await lookup(
        reference, libraryKey: \.id, entity: MusicEntity.album)
    case .artist:
      entity = try await lookup(
        reference, libraryKey: \.id, entity: MusicEntity.artist)
    case .playlist:
      entity = try await lookup(
        reference, libraryKey: \.id, entity: MusicEntity.playlist)
    case .station:
      guard reference.source == .catalog else { throw missing() }
      let station: Station = try await web.resource(reference)
      entity = .station(station)
    }
    try Task.checkCancellation()
    _ = remember(entity, source: reference.source)
    return entity
  }

  private func lookup<T: MusicLibraryRequestable & FilterableMusicItem & Decodable>(
    _ reference: ItemRef, libraryKey: KeyPath<T.LibraryFilter, MusicItemID>,
    entity: (T) -> MusicEntity
  ) async throws -> MusicEntity {
    let item: T?
    if reference.source == .library {
      var request = MusicLibraryRequest<T>()
      request.limit = 1
      request.filter(matching: libraryKey, equalTo: MusicItemID(reference.id))
      item = try await request.response().items.first
    } else {
      item = try await web.resource(reference)
    }
    guard let item else { throw missing() }
    return entity(item)
  }

  private func missing() -> Failure {
    .init(code: .notFound, message: "이 항목을 현재 보관함이나 지역에서 찾을 수 없습니다", retryable: false)
  }
}
