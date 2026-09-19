import Foundation
import MusicContract

/// Explicit fixture provider. No native player, authorization, volume or network objects.
struct DemoCatalog: Decodable {
  let tracks: [Item]
  let albums: [Item]
  let seed: SavedLibrary
  let player: PlayerState
  let volume: VolumeState
  let queue: [QueueEntry]
  let lyrics: [String]
  let cover: [UInt8]

  static func load() throws -> Self {
    guard let encoded = Data(base64Encoded: DemoAssets.payload, options: .ignoreUnknownCharacters)
    else {
      throw missing()
    }
    let data = try (encoded as NSData).decompressed(using: .zlib) as Data
    return try JSONDecoder().decode(Self.self, from: data)
  }

  func items(_ kind: Kind, source: Source) throws -> [Item] {
    let items: [Item]
    switch kind {
    case .song: items = tracks
    case .album: items = albums
    case .artist:
      items = albums.map { album in
        var artist = album
        artist.ref = album.artistRef!
        artist.title = album.artist
        return artist
      }
    case .playlist:
      items = [
        .init(
          ref: .init(id: "fixture-playlist-0", source: source, kind: .playlist),
          title: "Evening Mix", artist: "Music", album: "")
      ]
    case .station:
      guard source == .catalog else { throw Self.missing() }
      items = [
        .init(
          ref: .init(id: "fixture-station-0", source: source, kind: .station),
          title: "Night Radio", artist: "Music", album: "")
      ]
    }
    return items.map { original in
      var item = original
      item.ref.source = source
      item.albumRef?.source = source
      item.artistRef?.source = source
      return item
    }
  }

  func item(_ reference: ItemRef) throws -> Item {
    guard reference.source != .collection,
      let item = try items(reference.kind, source: reference.source).first(where: {
        $0.ref == reference
      })
    else { throw Self.missing() }
    return item
  }

  func songs(_ reference: ItemRef) throws -> [Item] {
    let item = try item(reference)
    if reference.kind == .song { return [item] }
    let songs = try items(.song, source: reference.source)
    switch reference.kind {
    case .album: return songs.filter { $0.albumRef == reference }
    case .artist: return songs.filter { $0.artistRef == reference }
    case .playlist, .station: return songs
    default: throw Self.missing()
    }
  }

  func search(_ params: SearchParams) throws -> Page {
    let matches = try items(params.kind, source: params.source).filter {
      [$0.title, $0.artist, $0.album].joined(separator: " ").localizedStandardContains(params.query)
    }
    return page(matches, offset: params.offset)
  }

  func browse(_ params: BrowseParams) throws -> Page {
    let home = params.scope == .home
    var result = try items(home ? .album : params.kind, source: home ? .catalog : .library)
    if params.order != .recent {
      result.sort {
        (params.order == .artist ? $0.artist : $0.title)
          .localizedStandardCompare(params.order == .artist ? $1.artist : $1.title)
          == .orderedAscending
      }
    }
    return page(result, offset: params.offset)
  }

  func detail(_ params: DetailParams) throws -> Detail {
    var item = try item(params.item)
    if item.ref.kind == .song, let album = item.albumRef { item = try self.item(album) }
    let children =
      item.ref.kind == .artist
      ? try items(.album, source: item.ref.source).filter { $0.artistRef == item.ref }
      : try songs(item.ref)
    let page = page(children, offset: params.offset)
    return .init(item: item, children: page.items, nextOffset: page.nextOffset)
  }

  func artwork(_ reference: ItemRef) throws -> Artwork {
    guard try item(reference).artworkUrl != nil, cover.count == 96 * 96 * 3 else {
      throw Self.missing()
    }
    return .init(item: reference, width: 96, height: 96, rgb: cover)
  }

  func lyric(_ reference: ItemRef, preference: LyricPreference = .init()) throws -> Lyrics {
    let item = try item(reference)
    let lines = lyrics
    return .init(
      item: reference, matchId: preference.matchId, status: .synced,
      lines: lines.enumerated().map {
        .init(
          seconds: Double($0.offset) * (item.duration ?? 208) / Double(lines.count),
          text: $0.element)
      },
      offset: preference.offset)
  }

  func page(_ items: [Item], offset: UInt64) -> Page {
    let start = Int(min(offset, UInt64(items.count)))
    let end = min(start + 50, items.count)
    return .init(
      items: Array(items[start..<end]), nextOffset: end < items.count ? UInt64(end) : nil)
  }

  static func missing() -> Failure {
    .init(code: .notFound, message: "항목을 찾을 수 없습니다", retryable: false)
  }
}
