import Foundation
import MusicContract
@preconcurrency import MusicKit

extension MusicService {
  func catalogDetail(_ params: DetailParams, limit: Int) async throws -> Detail {
    try requireAuthorization()
    let reference = params.item
    if reference.kind == .song {
      let song: Song = try await web.resource(reference, include: ["albums", "artists"])
      let item = remember(.song(song), source: .catalog)
      if let album = song.albums?.first {
        let parent = remember(.album(album), source: .catalog)
        return try await catalogDetail(.init(item: parent.ref, offset: params.offset), limit: limit)
      }
      return .init(item: item, children: params.offset == 0 ? [item] : [])
    }
    let heading = try await resolve(reference).item(source: .catalog)
    let page: Page
    switch reference.kind {
    case .album, .playlist:
      page = try await catalogChildren(params, relationship: "tracks", limit: limit) { (track: Track) in
        if case .song(let song) = track { return MusicEntity.song(song) }
        return nil
      }
    case .artist:
      page = try await catalogChildren(params, relationship: "albums", limit: limit, entity: MusicEntity.album)
    default: return .init(item: heading, children: [])
    }
    return .init(item: heading, children: page.items, nextOffset: page.nextOffset)
  }

  private func catalogChildren<T: Decodable>(
    _ params: DetailParams, relationship: String, limit: Int, entity: (T) -> MusicEntity?
  ) async throws -> Page {
    var offset = params.offset
    var items: [Item] = []
    // ponytail: cap a scan at 10,000 raw resources; raise only for a measured large-video playlist need.
    for _ in 0..<200 {
      let batchSize = min(50, limit - items.count)
      let page: WebPage<T> = try await web.relationship(
        params.item, name: relationship, offset: offset, limit: batchSize)
      try Task.checkCancellation()
      guard page.data.count <= batchSize else { throw AppleWeb.invalidResponse() }
      let next = try page.nextOffset(after: offset)
      for value in page.data {
        if let entity = entity(value) { items.append(remember(entity, source: .catalog)) }
      }
      if items.count == limit || next == nil { return .init(items: items, nextOffset: next) }
      offset = next!
    }
    throw Failure(code: .conflict, message: "한 번에 읽을 수 있는 목록 크기를 초과했습니다", retryable: false)
  }
}
