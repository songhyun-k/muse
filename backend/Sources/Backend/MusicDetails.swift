import Foundation
import MusicContract
@preconcurrency import MusicKit

extension MusicService {
  func detail(_ params: DetailParams, limit: Int = 50) async throws -> Detail {
    let source = params.item.source
    if source == .catalog { return try await catalogDetail(params, limit: limit) }
    let preferred: MusicPropertySource = .library
    let entity = try await resolve(params.item)
    let heading: MusicEntity
    let page: Page
    switch entity {
    case .song(var song):
      song = try await song.with([.albums, .artists], preferredSource: preferred)
      try Task.checkCancellation()
      let resolved = remember(.song(song), source: source)
      if let album = song.albums?.first {
        let item = remember(.album(album), source: source)
        return try await detail(.init(item: item.ref, offset: params.offset), limit: limit)
      }
      return .init(item: resolved, children: params.offset == 0 ? [resolved] : [])
    case .album(var album):
      if album.tracks == nil { album = try await album.with([.tracks], preferredSource: preferred) }
      heading = .album(album)
      page = try await relationshipPage(
        album.tracks, source: source, offset: params.offset, limit: limit, entity: songEntity)
    case .playlist(var playlist):
      if playlist.tracks == nil {
        playlist = try await playlist.with([.tracks], preferredSource: preferred)
      }
      heading = .playlist(playlist)
      page = try await relationshipPage(
        playlist.tracks, source: source, offset: params.offset, limit: limit, entity: songEntity)
    case .artist(var artist):
      if artist.albums == nil {
        artist = try await artist.with([.albums], preferredSource: preferred)
      }
      heading = .artist(artist)
      page = try await relationshipPage(
        artist.albums, source: source, offset: params.offset, limit: limit,
        entity: MusicEntity.album)
    case .station:
      return .init(item: entity.item(source: source), children: [])
    }
    try Task.checkCancellation()
    return .init(
      item: remember(heading, source: source), children: page.items, nextOffset: page.nextOffset)
  }

  func songs(_ reference: ItemRef) async throws -> [Item] {
    if reference.kind == .song {
      return [try await resolve(reference).item(source: reference.source)]
    }
    guard reference.kind == .album || reference.kind == .playlist else {
      throw Failure(code: .unavailable, message: "노래나 앨범, 플레이리스트를 선택해주세요", retryable: false)
    }
    // One relationship scan, including large playlists, rather than rescanning each page.
    let result = try await detail(.init(item: reference, offset: 0), limit: 2001)
    guard result.children.count <= 2000 && result.nextOffset == nil else {
      throw Failure(code: .conflict, message: "한 번에 2000곡까지 담을 수 있습니다", retryable: false)
    }
    return result.children
  }

  func relationshipPage<T: MusicItem & Decodable>(
    _ initial: MusicItemCollection<T>?,
    source: Source, offset: UInt64, limit: Int = 50, entity: (T) -> MusicEntity?
  ) async throws -> Page {
    guard var batch = initial else { return .init(items: []) }
    var position: UInt64 = 0
    var items: [Item] = []
    while true {
      try Task.checkCancellation()
      for (index, value) in batch.enumerated() {
        if position >= offset, let value = entity(value) {
          items.append(remember(value, source: source))
        }
        position += 1
        if items.count == limit {
          let more = index + 1 < batch.count || batch.hasNextBatch
          return .init(items: items, nextOffset: more ? position : nil)
        }
      }
      guard batch.hasNextBatch, let next = try await batch.nextBatch(limit: 50), !next.isEmpty
      else { break }
      batch = next
    }
    return .init(items: items)
  }

  private func songEntity(_ track: Track) -> MusicEntity? {
    if case .song(let song) = track { return .song(song) }
    return nil
  }

  func recommendations(offset: UInt64) async throws -> Page {
    try requireAuthorization()
    let response = try await web.recommendations()
    try Task.checkCancellation()
    // Home is a curated selection of up to 50 recommendations, not the whole catalog.
    let candidates = response.recommendations.flatMap(\.items).prefix(50)
    var seen: Set<String> = []
    let items = candidates.compactMap { value -> Item? in
      let entity: MusicEntity
      switch value {
      case .album(let album): entity = .album(album)
      case .playlist(let playlist): entity = .playlist(playlist)
      case .station(let station): entity = .station(station)
      @unknown default: return nil
      }
      let item = remember(entity, source: .catalog)
      return seen.insert(item.ref.storageKey).inserted ? item : nil
    }
    return .init(items: Array(items.dropFirst(Int(offset))))
  }
}
