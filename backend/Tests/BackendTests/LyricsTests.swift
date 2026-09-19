import Foundation
import MusicContract
import Testing

@testable import Backend

private final class LyricsStub: URLProtocol, @unchecked Sendable {
  override class func canInit(with request: URLRequest) -> Bool { true }
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
  override func startLoading() {
    let query = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)?.queryItems
    let title = query?.first(where: { $0.name == "track_name" })?.value
    let status = title == "missing" ? 404 : title == "failed" ? 503 : 200
    var data = Data(
      #"{"id":42,"instrumental":false,"plainLyrics":"fixture","syncedLyrics":"[00:01.50]테스트"}"#.utf8
    )
    if request.url?.path == "/api/search" { data = Data("[".utf8) + data + Data("]".utf8) }
    let response = HTTPURLResponse(
      url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!
    client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
    client?.urlProtocol(self, didLoad: data)
    client?.urlProtocolDidFinishLoading(self)
  }
  override func stopLoading() {}
}

@Test func lrcParsesMultipleTagsAndKeepsStableOrdering() {
  let lines = LRC.parse(
    "[ar:fixture]\n[00:01.50][00:03]첫 줄\n[00:02.250]<00:02.25>두 번째\n[00:99]invalid\n[00:04] ")
  #expect(lines.map(\.seconds) == [1.5, 2.25, 3, 4])
  #expect(lines.map(\.text) == ["첫 줄", "두 번째", "첫 줄", ""])
  #expect(LRC.parse("ordinary text").isEmpty)
}

@Test func lyricsKeepPlainInstrumentalAndMissingDistinct() {
  let ref = sampleSong("lyrics").ref
  var record = LyricRecord(id: 1, instrumental: false, plainLyrics: "plain\nline")
  #expect(record.lyrics(for: ref).status == .plain)
  record.instrumental = true
  #expect(record.lyrics(for: ref).status == .instrumental)
  record.instrumental = false
  record.plainLyrics = nil
  #expect(record.lyrics(for: ref).status == .missing)
}

@MainActor @Test func providerMissingAndFailureAreNotConfused() async throws {
  let configuration = URLSessionConfiguration.ephemeral
  configuration.protocolClasses = [LyricsStub.self]
  let session = URLSession(configuration: configuration)
  defer { session.invalidateAndCancel() }
  let service = LyricsService(session: session)
  var item = sampleSong("synced")
  item.title = "한글 & + 검색"
  let synced = try await service.load(item)
  #expect(synced.status == .synced && synced.matchId == 42)
  let original = item
  item = sampleSong("missing")
  item.title = "missing"
  #expect(try await service.load(item).status == .missing)
  item = sampleSong("failed")
  item.title = "failed"
  await #expect(throws: Failure.self) { try await service.load(item) }
  session.invalidateAndCancel()
  let offset = try await service.load(original, preference: .init(offset: 0.5))
  #expect(offset.offset == 0.5 && offset.lines == synced.lines)
}

@MainActor @Test(.enabled(if: ProcessInfo.processInfo.environment["MUSIC_LIVE_LYRICS"] == "1"))
func liveLyricsLookup() async throws {
  // Public provider acceptance only; this does not claim MusicKit authorization.
  let item = Item(
    ref: .init(id: "lrclib-acceptance", source: .catalog, kind: .song),
    title: "Never Gonna Give You Up", artist: "Rick Astley", album: "Whenever You Need Somebody",
    duration: 213)
  let lyrics = try await LyricsService().load(item)
  #expect(lyrics.matchId != nil)
  #expect(lyrics.status == .synced || lyrics.status == .plain)
  #expect(!lyrics.lines.isEmpty)
}

@MainActor @Test func selectedLyricsAndOffsetAreSavedOnlyAfterSuccess() async throws {
  let configuration = URLSessionConfiguration.ephemeral
  configuration.protocolClasses = [LyricsStub.self]
  let session = URLSession(configuration: configuration)
  defer { session.invalidateAndCancel() }
  let service = Service(lyricsSession: session)
  let item = sampleSong("manual")
  try service.store.get().favorite(item, enabled: true)
  let search = Request(
    version: 1, id: 1, command: .lyricsSearch(.init(item: item.ref, query: "한글 & +")))
  let found = await service.handle(try Wire.encode(search))
  guard case .lyricsMatches(let matches) = found.event else {
    Issue.record("Expected lyric matches")
    return
  }
  #expect(matches.matches.first?.id == 42 && matches.matches.first?.duration == nil)
  let choose = Request(
    version: 1, id: 2, command: .lyricsChoose(.init(item: item.ref, matchId: 42)))
  let chosen = await service.handle(try Wire.encode(choose))
  guard case .lyrics(let lyrics) = chosen.event else {
    Issue.record("Expected selected lyrics")
    return
  }
  #expect(lyrics.matchId == 42 && lyrics.status == .synced)
  let adjust = Request(
    version: 1, id: 3, command: .lyricsOffset(.init(item: item.ref, seconds: -0.25)))
  let adjusted = await service.handle(try Wire.encode(adjust))
  guard case .lyrics(let updated) = adjusted.event else {
    Issue.record("Expected offset")
    return
  }
  #expect(updated.offset == -0.25)
  #expect(
    try service.store.get().state.lyrics[item.ref.storageKey] == .init(matchId: 42, offset: -0.25))
  let absent = Request(
    version: 1, id: 4, command: .lyricsChoose(.init(item: item.ref, matchId: 999)))
  let failed = await service.handle(try Wire.encode(absent))
  guard case .failure = failed.event else {
    Issue.record("Expected failed selection")
    return
  }
  #expect(try service.store.get().state.lyrics[item.ref.storageKey]?.matchId == 42)
  service.close()
}
