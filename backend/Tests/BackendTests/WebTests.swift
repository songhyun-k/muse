import Foundation
import MusicContract
import MusicKit
import Testing
@testable import Backend

struct WebReply {
  var status = 200
  var body: Data
  var headers: [String: String] = [:]
  init(_ text: String, status: Int = 200, headers: [String: String] = [:]) {
    self.status = status
    body = Data(text.utf8)
    self.headers = headers
  }
}

final class WebStubState: @unchecked Sendable {
  private let lock = NSLock()
  private var replies: [String: [WebReply]] = [:]
  private var requests: [URLRequest] = []
  private var held: Set<String> = []
  func reset(_ replies: [String: WebReply], held: Set<String> = []) {
    lock.withLock { self.replies = replies.mapValues { [$0] }; self.held = held; requests = [] }
  }
  func sequence(_ path: String, _ values: [WebReply]) { lock.withLock { replies[path] = values } }
  func reply(_ request: URLRequest) -> WebReply? {
    lock.withLock {
      requests.append(request)
      if held.contains(request.url!.path) { return nil }
      let path = request.url!.path
      if let values = replies[path], values.count > 1 { return replies[path]!.removeFirst() }
      return replies[path]?.first ?? WebReply("unexpected request", status: 500)
    }
  }
  var seen: [URLRequest] { lock.withLock { requests } }
}

final class WebStub: URLProtocol, @unchecked Sendable {
  static let state = WebStubState()
  override class func canInit(with request: URLRequest) -> Bool { true }
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
  override func startLoading() {
    guard let reply = Self.state.reply(request) else { return }
    let response = HTTPURLResponse(url: request.url!, statusCode: reply.status,
      httpVersion: nil, headerFields: reply.headers)!
    if let location = reply.headers["Location"], let url = URL(string: location) {
      client?.urlProtocol(self, wasRedirectedTo: URLRequest(url: url), redirectResponse: response)
    }
    client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
    client?.urlProtocol(self, didLoad: reply.body)
    client?.urlProtocolDidFinishLoading(self)
  }
  override func stopLoading() {}
}

func webConfiguration() -> URLSessionConfiguration {
  let configuration = URLSessionConfiguration.ephemeral
  configuration.protocolClasses = [WebStub.self]
  return configuration
}

func webTestToken(expiry: Double, issuer: String = "AMPWebPlay") -> String {
  func encode(_ value: [String: Any]) -> String {
    try! JSONSerialization.data(withJSONObject: value, options: .sortedKeys).base64EncodedString()
      .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
  }
  return encode(["alg": "ES256"]) + "." + encode(["iss": issuer, "exp": expiry]) + ".c2ln"
}

@Suite(.serialized) @MainActor
struct WebTests {
  @Test func missingLoginOpensMusicOnceAndRetriesAfterSignIn() async throws {
    let token = webTestToken(expiry: Date().timeIntervalSince1970 + 3600)
    WebStub.state.reset([
      "/us/new": WebReply("let token='\(token)'"),
      "/v1/me/storefront": WebReply(#"{"data":[{"id":"us"}]}"#),
      "/v1/catalog/us/search": WebReply(#"{"results":{"songs":{"data":[]}}}"#)
    ])
    var signedIn = false
    var opened = 0
    var offline = false
    let web = WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in
      if offline { throw URLError(.notConnectedToInternet) }
      guard signedIn else { throw MusicTokenRequestError.userNotSignedIn }
      return "fixture-user"
    }
    let service = Service(music: MusicService(web: web, authorization: { .authorized }),
                          openMusic: { opened += 1 })
    defer { service.close() }
    for id: UInt64 in 1...5 {
      signedIn = id == 3
      offline = id == 5
      let event = await service.handle(try Wire.encode(Request(version: 1, id: id,
        command: .search(.init(query: "Mira", source: .catalog, kind: .song, offset: 0)))))
      if signedIn {
        guard case .page = event.event else { Issue.record("Login did not recover search"); return }
      } else {
        guard case .failure(let failure) = event.event else { Issue.record("Missing failure"); return }
        #expect(failure.code == (offline ? .network : .signInRequired))
        #expect(failure.retryable)
      }
      #expect(opened == (id < 4 ? 1 : 2))
    }
  }

  @Test func tokenAndAssetParsingRejectsExpiredForeignAndMalformedInputs() {
    let now = Date(timeIntervalSince1970: 1000)
    let valid = webTestToken(expiry: 2000)
    let expired = webTestToken(expiry: 1050)
    let foreign = webTestToken(expiry: 2000, issuer: "other")
    #expect(AppleWeb.extractToken(expired + foreign, now: now) == nil)
    let found = AppleWeb.extractToken("let old='\(expired)';let current='\(valid)'", now: now)
    #expect(found?.value == valid)
    #expect(found?.validUntil == Date(timeIntervalSince1970: 1940))
    #expect(AppleWeb.extractToken("eyJbroken.eyJbroken.signature", now: now) == nil)
    let html = """
      <script src="https://evil.test/assets/index-x.js"></script>
      <script src="/assets/index/../../bad.js"></script>
      <script src="/assets/index-one.js"></script><script src='/assets/index-one.js'></script>
      <script src='/assets/index-two.js'></script><script src='/assets/index-three.js'></script>
      """
    #expect(AppleWeb.assets(html).map(\.path) == ["/assets/index-one.js", "/assets/index-two.js"])
  }

  @Test func publicTokenFetchCoalescesCachesInvalidatesAndCloses() async throws {
    let token = webTestToken(expiry: Date().timeIntervalSince1970 + 3600)
    WebStub.state.reset([
      "/us/new": WebReply("<script src='/assets/index-main.js'></script>"),
      "/assets/index-main.js": WebReply("const token='\(token)'")
    ])
    let web = AppleWeb(configuration: webConfiguration())
    async let first = web.token()
    async let second = web.token()
    let values = try await [first.value, second.value]
    #expect(values == [token, token])
    #expect(WebStub.state.seen.count == 2)
    _ = try await web.token()
    web.invalidate("a superseded credential")
    _ = try await web.token()
    #expect(WebStub.state.seen.count == 2)
    web.invalidate(token)
    _ = try await web.token()
    #expect(WebStub.state.seen.count == 4)
    web.close()
    await #expect(throws: CancellationError.self) { try await web.token() }
  }

  @Test func HTTPRejectsOversizedBodiesUnsafeDestinationsAndRedirects() async throws {
    WebStub.state.reset([
      "/v1/large": WebReply("0123456789"),
      "/v1/declared": WebReply("tiny", headers: ["Content-Length": "100"]),
      "/v1/redirect": WebReply("", status: 302, headers: ["Location": "https://evil.test/steal"])
    ])
    let web = AppleWeb(configuration: webConfiguration())
    defer { web.close() }
    for text in ["http://amp-api.music.apple.com/v1/x", "https://evil.test/v1/x",
                 "https://user:pass@amp-api.music.apple.com/v1/x", "https://music.apple.com/assets/x"] {
      await #expect(throws: Failure.self) {
        try await web.load(URL(string: text)!, headers: ["Media-User-Token": "test"], maximum: 8)
      }
    }
    #expect(WebStub.state.seen.isEmpty)
    for path in ["large", "declared"] {
      await #expect(throws: Failure.self) {
        try await web.load(URL(string: "https://amp-api.music.apple.com/v1/\(path)")!, maximum: 8)
      }
    }
    do {
      _ = try await web.load(URL(string: "https://amp-api.music.apple.com/v1/redirect")!,
                            headers: ["Media-User-Token": "test"], maximum: 8)
      Issue.record("redirect was accepted")
    } catch let failure as WebHTTPFailure { #expect(failure.status == 302) }
    #expect(WebStub.state.seen.allSatisfy { $0.url?.host == "amp-api.music.apple.com" })
  }

  @Test func closingCancelsAnInFlightTokenDownload() async throws {
    WebStub.state.reset([:], held: ["/us/new"])
    let web = AppleWeb(configuration: webConfiguration())
    defer { web.close() }
    let request = Task { try await web.token() }
    for _ in 0..<200 where WebStub.state.seen.isEmpty {
      try await Task.sleep(for: .milliseconds(5))
    }
    try #require(!WebStub.state.seen.isEmpty)
    web.close()
    do {
      _ = try await request.value
      Issue.record("closed token fetch succeeded")
    } catch {
      let cancelled = error is CancellationError || (error as? URLError)?.code == .cancelled
      #expect(cancelled)
    }
  }

  @Test(.enabled(if: ProcessInfo.processInfo.environment["MUSE_LIVE_WEB"] == "1"))
  func livePublicTokenWithoutAccount() async throws {
    let web = AppleWeb()
    defer { web.close() }
    let token = try await web.token()
    let valid = !token.value.isEmpty && token.validUntil > Date()
    #expect(valid)
  }
}

func webSong(_ id: String) -> String {
  #"{"id":"\#(id)","type":"songs","attributes":{"name":"Song \#(id)","artistName":"Artist","albumName":"Album","genreNames":["Pop"],"durationInMillis":60000,"playParams":{"id":"\#(id)","kind":"song"}}}"#
}

extension WebTests {
  @Test func publicBootstrapFailuresRemainActionableThroughService() async throws {
    let cases: [(Int, ErrorCode, String, Command)] = [
      (429, .busy, "잠시 후", .search(.init(query: "x", source: .catalog, kind: .song, offset: 0))),
      (503, .network, "연결", .browse(.init(scope: .home, kind: .album, order: .recent, offset: 0)))
    ]
    for (status, code, hint, command) in cases {
      WebStub.state.reset(["/us/new": WebReply("private provider payload", status: status)])
      let web = WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in
        Issue.record("failed public bootstrap reached account credentials")
        return "unused"
      }
      let service = Service(music: MusicService(web: web, authorization: { .authorized }))
      defer { service.close() }
      let event = await service.handle(try Wire.encode(Request(version: 1, id: 1, command: command)))
      guard case .failure(let failure) = event.event else { Issue.record("Expected failure"); continue }
      #expect(event.id == 1 && failure.code == code && failure.retryable)
      #expect(failure.message.contains(hint) && !failure.message.contains("private provider payload"))
      #expect(WebStub.state.seen.count == 1)
    }
  }

  @Test func webReadsRetainPlayableModelsQueryEncodingAndAccountStorefront() async throws {
    let token = webTestToken(expiry: Date().timeIntervalSince1970 + 3600)
    WebStub.state.reset([
      "/us/new": WebReply(token),
      "/v1/me/storefront": WebReply(#"{"data":[{"id":"jp"}]}"#),
      "/v1/catalog/jp/search": WebReply(#"{"results":{"songs":{"data":[\#(webSong("one"))],"next":"/v1/catalog/jp/search?offset=26"}}}"#),
      "/v1/catalog/jp/songs": WebReply(#"{"data":[\#(webSong("one"))]}"#),
      "/v1/me/recommendations": WebReply(#"{"data":[]}"#)
    ])
    let web = WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in "native-user" }
    defer { web.close() }
    let query = " 한글 & Ado "
    let page: WebPage<Song> = try await web.search(.init(query: query, source: .catalog, kind: .song, offset: 25))
    #expect(page.data.first?.id.rawValue == "one")
    #expect(page.data.first?.playParameters != nil)
    #expect(try page.nextOffset(after: 25) == 26)
    let song: Song = try await web.resource(.init(id: "one", source: .catalog, kind: .song))
    #expect(song.playParameters != nil)
    #expect(try await web.recommendations().recommendations.isEmpty)
    let requests = WebStub.state.seen
    #expect(requests.filter { $0.url?.path == "/v1/me/storefront" }.count == 1)
    let search = try #require(requests.first { $0.url?.path.hasSuffix("/search") == true })
    let terms = URLComponents(url: search.url!, resolvingAgainstBaseURL: false)!.queryItems!
    #expect(terms.first { $0.name == "term" }?.value == query.trimmingCharacters(in: .whitespacesAndNewlines))
    #expect(search.value(forHTTPHeaderField: "Media-User-Token") == "native-user")
    #expect(search.value(forHTTPHeaderField: "Origin") == "https://music.apple.com")
  }

  @Test func authRefreshReplacesBothCredentialsAndRechecksStorefrontOnlyOnce() async throws {
    let first = webTestToken(expiry: Date().timeIntervalSince1970 + 3600)
    let second = webTestToken(expiry: Date().timeIntervalSince1970 + 7200)
    WebStub.state.reset([
      "/v1/catalog/jp/search": WebReply("expired", status: 401),
      "/v1/catalog/us/search": WebReply(#"{"results":{"songs":{"data":[]}}}"#)
    ])
    WebStub.state.sequence("/us/new", [WebReply(first), WebReply(second)])
    WebStub.state.sequence("/v1/me/storefront", [
      WebReply(#"{"data":[{"id":"jp"}]}"#), WebReply(#"{"data":[{"id":"us"}]}"#)
    ])
    var requested: [(String, Bool)] = []
    let web = WebMusic(http: AppleWeb(configuration: webConfiguration())) { token, refresh in
      requested.append((token, refresh))
      return refresh ? "new-user" : "old-user"
    }
    defer { web.close() }
    let params = SearchParams(query: "x", source: .catalog, kind: .song, offset: 0)
    let page: WebPage<Song> = try await web.search(params)
    #expect(page.data.isEmpty)
    #expect(requested.map(\.0) == [first, second])
    #expect(requested.map(\.1) == [false, true])
    let final = try #require(WebStub.state.seen.last)
    #expect(final.value(forHTTPHeaderField: "Authorization") == "Bearer " + second)
    #expect(final.value(forHTTPHeaderField: "Media-User-Token") == "new-user")
    WebStub.state.sequence("/v1/catalog/us/search", [WebReply("still invalid", status: 403)])
    WebStub.state.sequence("/v1/me/storefront", [WebReply(#"{"data":[{"id":"us"}]}"#)])
    let before = requested.count
    await #expect(throws: Failure.self) { let _: WebPage<Song> = try await web.search(params) }
    #expect(requested.count - before == 2)
  }

  @Test func constructingServiceAndSnapshotNeverFetchesWebCredentials() async throws {
    WebStub.state.reset([:])
    let web = WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in
      Issue.record("cold snapshot requested user credentials")
      return "unused"
    }
    let service = Service(music: MusicService(web: web, authorization: { .authorized }))
    defer { service.close() }
    let result = await service.handle(try Wire.encode(Request(version: 1, id: 1, command: .snapshot(.init()))))
    guard case .snapshot = result.event else { Issue.record("snapshot failed"); return }
    #expect(WebStub.state.seen.isEmpty)
  }

  @Test func failedStoreStillPublishesIndependentBootstrapState() async throws {
    for corrupt in [false, true] {
      let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
      try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
      defer { try? FileManager.default.removeItem(at: directory) }
      let file = directory.appendingPathComponent("library.json")
      let owner = corrupt ? nil : try LibraryStore(file: file)
      defer { withExtendedLifetime(owner) {} }
      if corrupt {
        try Data("{broken".utf8).write(to: file)
      } else {
        _ = try owner?.create(name: "Preserved", description: "")
      }
      let original = try Data(contentsOf: file)
      WebStub.state.reset([:])
      let web = WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in
        Issue.record("bootstrap requested user credentials")
        return "unused"
      }
      let music = MusicService(web: web, authorization: { .authorized })
      let service = Service(storeURL: file, music: music)
      defer { service.close() }
      var events: [Event] = []
      service.onEvent = { events.append($0) }
      let reply = await service.handle(
        try Wire.encode(Request(version: 1, id: 1, command: .snapshot(.init()))))
      guard case .failure(let failure) = reply.event else {
        Issue.record("Expected the store failure, not a successful empty store")
        continue
      }
      #expect(reply.id == 1)
      #expect(failure.code == (corrupt ? .storage : .conflict))
      #expect(failure.retryable)
      #expect(events.map(\.event.tag) == ["session", "player", "volume"])
      #expect(events.allSatisfy { $0.id == nil && $0.sequence < reply.sequence })
      for event in events {
        switch event.event {
        case .session(let state): #expect(state == music.session())
        case .player(let state): #expect(!state.playing && state.current == nil)
        case .volume(let state):
          #expect(!state.canSetVolume || state.level != nil)
          #expect(!state.canMute || state.muted != nil)
        default: Issue.record("Unexpected bootstrap state")
        }
      }
      let mutation = await service.handle(try Wire.encode(Request(version: 1, id: 2,
        command: .collectionCreate(.init(name: "Must fail", description: "")))))
      #expect(mutation.event == .failure(failure))
      #expect(try Data(contentsOf: file) == original)
      #expect(music.player == nil)
      #expect(WebStub.state.seen.isEmpty)
    }
  }
}

func webAlbum(_ id: String) -> String {
  #"{"id":"\#(id)","type":"albums","attributes":{"name":"Album","artistName":"Artist","genreNames":["Pop"],"trackCount":53}}"#
}

extension WebTests {
  @Test func serviceSearchHomeAndSongAssociationsUseWebFromTheFirstPage() async throws {
    let token = webTestToken(expiry: Date().timeIntervalSince1970 + 3600)
    let song = String(webSong("one").dropLast()) + #", "relationships":{"albums":{"data":[\#(webAlbum("album"))]},"artists":{"data":[{"id":"artist","type":"artists","attributes":{"name":"Artist"}}]}}}"#
    let home = #"{"data":[{"id":"r","type":"personal-recommendation","attributes":{"title":{"stringForDisplay":"For you"},"resourceTypes":["albums"]},"relationships":{"contents":{"data":[\#(webAlbum("album"))]}}}]}"#
    WebStub.state.reset([
      "/us/new": WebReply(token), "/v1/me/storefront": WebReply(#"{"data":[{"id":"jp"}]}"#),
      "/v1/catalog/jp/search": WebReply(#"{"results":{"songs":{"data":[\#(webSong("one"))],"next":"?offset=25"}}}"#),
      "/v1/catalog/jp/songs": WebReply(#"{"data":[\#(song)]}"#),
      "/v1/catalog/jp/albums/album/tracks": WebReply(#"{"data":[\#(webSong("one"))]}"#),
      "/v1/me/recommendations": WebReply(home)
    ])
    let music = MusicService(web: WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in "user" },
                             authorization: { .authorized })
    let service = Service(music: music)
    defer { service.close() }
    var id: UInt64 = 0
    func call(_ command: Command) async throws -> Notice {
      id += 1
      let response = await service.handle(try Wire.encode(Request(version: 1, id: id, command: command)))
      if case .failure(let error) = response.event { throw error }
      return response.event
    }
    guard case .page(let search) = try await call(.search(.init(query: "one", source: .catalog, kind: .song, offset: 0)))
    else { Issue.record("search failed"); return }
    #expect(search.items.first?.ref.id == "one" && search.nextOffset == 25)
    let enriched: Song = try await music.web.resource(search.items[0].ref, include: ["albums", "artists"])
    #expect(enriched.albums?.first?.id.rawValue == "album")
    #expect(enriched.artists?.first?.id.rawValue == "artist")
    guard case .detail(let detail) = try await call(.detail(.init(item: search.items[0].ref, offset: 0)))
    else { Issue.record("detail failed"); return }
    #expect(detail.item.ref.id == "album" && detail.children.first?.ref.id == "one")
    if case .song(let native) = music.entities[search.items[0].ref.storageKey] {
      #expect(native.playParameters != nil)
    } else { Issue.record("native song missing") }
    guard case .page(let recommendations) = try await call(.browse(.init(scope: .home, kind: .album, order: .recent, offset: 0)))
    else { Issue.record("Home failed"); return }
    #expect(recommendations.items.first?.ref.id == "album")
    #expect(music.player == nil)
  }

  @Test func albumAndPlaylistPlaybackExpansionNeverTruncatesWebPages() async throws {
    for kind in [Kind.album, .playlist] {
      let token = webTestToken(expiry: Date().timeIntervalSince1970 + 3600)
      let parent = kind == .album ? webAlbum("parent") : #"{"id":"parent","type":"playlists","attributes":{"name":"Playlist","playlistType":"editorial"}}"#
      let path = "/v1/catalog/jp/\(kind.rawValue)s/parent/tracks"
      let first = (0..<50).map { webSong(String($0)) }.joined(separator: ",")
      let second = (50..<53).map { webSong(String($0)) }.joined(separator: ",")
      WebStub.state.reset([
        "/us/new": WebReply(token), "/v1/me/storefront": WebReply(#"{"data":[{"id":"jp"}]}"#),
        "/v1/catalog/jp/\(kind.rawValue)s": WebReply(#"{"data":[\#(parent)]}"#)
      ])
      WebStub.state.sequence(path, [WebReply(#"{"data":[\#(first)],"next":"?offset=50"}"#),
                                   WebReply(#"{"data":[\#(second)]}"#)])
      let music = MusicService(web: WebMusic(http: AppleWeb(configuration: webConfiguration())) { _, _ in "user" },
                               authorization: { .authorized })
      defer { music.web.close() }
      let reference = ItemRef(id: "parent", source: .catalog, kind: kind)
      let songs = try await music.songs(reference)
      #expect(songs.map(\.ref.id) == (0..<53).map(String.init))
      #expect(WebStub.state.seen.filter { $0.url?.path == path }.count == 2)
      WebStub.state.sequence(path, [WebReply(#"{"data":[\#(first)],"next":"?offset=0"}"#)])
      await #expect(throws: Failure.self) { try await music.songs(reference) }
    }
  }
}
