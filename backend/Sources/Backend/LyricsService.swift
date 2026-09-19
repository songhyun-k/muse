import Foundation
import MusicContract

struct LyricRecord: Decodable {
  var id: UInt64
  var name: String?
  var trackName: String?
  var artistName: String?
  var albumName: String?
  var duration: Double?
  var instrumental: Bool
  var plainLyrics: String?
  var syncedLyrics: String?

  func lyrics(for reference: ItemRef) -> Lyrics {
    if instrumental {
      return .init(item: reference, matchId: id, status: .instrumental, lines: [], offset: 0)
    }
    let synced = LRC.parse(syncedLyrics ?? "")
    if !synced.isEmpty {
      return .init(item: reference, matchId: id, status: .synced, lines: synced, offset: 0)
    }
    let plain = (plainLyrics ?? "").components(separatedBy: .newlines)
    guard plain.contains(where: { !$0.trimmingCharacters(in: .whitespaces).isEmpty }) else {
      return .init(item: reference, matchId: id, status: .missing, lines: [], offset: 0)
    }
    return .init(
      item: reference, matchId: id, status: .plain,
      lines: plain.map { .init(text: $0) }, offset: 0)
  }
}

enum LRC {
  static func parse(_ text: String) -> [LyricLine] {
    let timestamp = /\[([0-9]{1,3}):([0-9]{1,2}(?:\.[0-9]{1,3})?)\]/
    let wordTime = /<[0-9]{1,3}:[0-9]{1,2}(?:\.[0-9]{1,3})?>/
    var result: [LyricLine] = []
    for line in text.components(separatedBy: .newlines) {
      let tags = line.matches(of: timestamp)
      guard let end = tags.last?.range.upperBound else { continue }
      let words = String(line[end...]).replacing(wordTime, with: "").trimmingCharacters(
        in: .whitespaces)
      for tag in tags {
        guard let minutes = Double(tag.1), let seconds = Double(tag.2), seconds < 60 else {
          continue
        }
        result.append(.init(seconds: minutes * 60 + seconds, text: words))
      }
    }
    return result.enumerated().sorted {
      let left = $0.element.seconds ?? 0
      let right = $1.element.seconds ?? 0
      return left == right ? $0.offset < $1.offset : left < right
    }.map(\.element)
  }
}

@MainActor
final class LyricsService {
  private struct Cached {
    let time: Date
    let selection: UInt64?
    let lyrics: Lyrics
  }
  private let session: URLSession
  private var cache: [String: Cached] = [:]

  init(session: URLSession = .shared) { self.session = session }

  func search(_ query: String, for reference: ItemRef) async throws -> LyricsMatches {
    guard let data = try await request("search", query: [.init(name: "q", value: query)]) else {
      throw networkFailure()
    }
    let records = try decode(data, as: [LyricRecord].self)
    let matches = records.prefix(50).map { record in
      LyricMatch(
        id: record.id, title: record.trackName ?? record.name ?? "제목 없음",
        artist: record.artistName ?? "", album: record.albumName ?? "", duration: record.duration,
        synced: !(record.syncedLyrics ?? "").isEmpty)
    }
    return .init(item: reference, matches: matches)
  }

  func remember(_ record: LyricRecord, for reference: ItemRef, offset: Double) throws -> Lyrics {
    var lyrics = record.lyrics(for: reference)
    do { _ = try Wire.encode(lyrics) } catch { throw networkFailure() }
    if cache.count >= 64 { cache.removeAll(keepingCapacity: true) }
    cache[reference.storageKey] = Cached(time: Date(), selection: record.id, lyrics: lyrics)
    lyrics.offset = offset
    return lyrics
  }

  func load(_ item: Item, preference: LyricPreference = .init()) async throws -> Lyrics {
    if let cached = cache[item.ref.storageKey], cached.selection == preference.matchId,
      Date().timeIntervalSince(cached.time) < 1800
    {
      var lyrics = cached.lyrics
      lyrics.offset = preference.offset
      return lyrics
    }
    guard item.ref.kind == .song else {
      throw Failure(code: .unavailable, message: "노래를 선택해주세요", retryable: false)
    }
    let record: LyricRecord?
    if let id = preference.matchId {
      record = try await self.record(id)
    } else {
      var query = [
        URLQueryItem(name: "track_name", value: item.title),
        URLQueryItem(name: "artist_name", value: item.artist),
      ]
      if !item.album.isEmpty { query.append(.init(name: "album_name", value: item.album)) }
      if let duration = item.duration, (1...3600).contains(duration) {
        query.append(.init(name: "duration", value: String(duration)))
      }
      let data = try await request("get", query: query, allowMissing: true)
      record = try data.map { try decode($0, as: LyricRecord.self) }
    }
    try Task.checkCancellation()
    var lyrics =
      record?.lyrics(for: item.ref) ?? .init(item: item.ref, status: .missing, lines: [], offset: 0)
    do { _ = try Wire.encode(lyrics) } catch {
      throw Failure(code: .network, message: "가사 데이터를 읽을 수 없습니다", retryable: true)
    }
    if cache.count >= 64 { cache.removeAll(keepingCapacity: true) }
    cache[item.ref.storageKey] = Cached(time: Date(), selection: preference.matchId, lyrics: lyrics)
    lyrics.offset = preference.offset
    return lyrics
  }

  func record(_ id: UInt64) async throws -> LyricRecord? {
    let data = try await request("get/\(id)", allowMissing: true)
    return try data.map { try decode($0, as: LyricRecord.self) }
  }

  func decode<T: Decodable>(_ data: Data, as type: T.Type) throws -> T {
    do { return try JSONDecoder().decode(type, from: data) } catch { throw networkFailure() }
  }

  func request(_ path: String, query: [URLQueryItem] = [], allowMissing: Bool = false) async throws
    -> Data?
  {
    var components = URLComponents()
    components.scheme = "https"
    components.host = "lrclib.net"
    components.path = "/api/" + path
    if !query.isEmpty { components.queryItems = query }
    guard let url = components.url else {
      throw Failure(code: .invalidRequest, message: "가사 검색어를 확인해주세요", retryable: false)
    }
    var request = URLRequest(url: url, timeoutInterval: 10)
    request.setValue("muse", forHTTPHeaderField: "User-Agent")
    let (bytes, response) = try await session.bytes(for: request)
    defer { bytes.task.cancel() }
    guard let response = response as? HTTPURLResponse else { throw networkFailure() }
    if allowMissing && response.statusCode == 404 { return nil }
    guard (200..<300).contains(response.statusCode) else { throw networkFailure() }
    return try await boundedBody(
      bytes, response: response, maximum: 1024 * 1024, failure: networkFailure())
  }

  private func networkFailure() -> Failure {
    .init(code: .network, message: "가사를 불러오지 못했습니다. 다시 시도해주세요", retryable: true)
  }
}
