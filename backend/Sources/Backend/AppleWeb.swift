import Foundation
import MusicContract

struct WebHTTPFailure: Error { let status: Int }

private final class NoWebRedirects: NSObject, URLSessionTaskDelegate, Sendable {
  func urlSession(
    _ session: URLSession, task: URLSessionTask,
    willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
    completionHandler: @escaping @Sendable (URLRequest?) -> Void
  ) { completionHandler(nil) }
}

@MainActor
final class AppleWeb {
  struct Token {
    let value: String
    let validUntil: Date
  }

  private let session: URLSession
  private var cached: Token?
  private var pending: Task<Token, Error>?
  private var closed = false

  init(configuration: URLSessionConfiguration = .ephemeral) {
    configuration.urlCache = nil
    configuration.httpCookieStorage = nil
    configuration.urlCredentialStorage = nil
    configuration.httpShouldSetCookies = false
    configuration.timeoutIntervalForRequest = 10
    configuration.timeoutIntervalForResource = 15
    session = URLSession(configuration: configuration)
  }

  func close() {
    closed = true
    pending?.cancel()
    pending = nil
    cached = nil
    session.invalidateAndCancel()
  }

  func invalidate(_ value: String) {
    if cached?.value == value { cached = nil }
  }

  func token() async throws -> Token {
    try Task.checkCancellation()
    guard !closed else { throw CancellationError() }
    if let cached, cached.validUntil > Date() { return cached }
    if let pending {
      let value = try await pending.value
      try Task.checkCancellation()
      return value
    }
    let task = Task { try await self.fetchToken() }
    pending = task
    defer { pending = nil }
    let value = try await task.value
    guard !closed else { throw CancellationError() }
    cached = value
    try Task.checkCancellation()
    return value
  }

  func load(_ url: URL, headers: [String: String] = [:], maximum: Int) async throws -> Data {
    try Task.checkCancellation()
    guard !closed else { throw CancellationError() }
    guard url.scheme == "https", url.user == nil, url.password == nil, url.port == nil,
      url.host == "music.apple.com" || url.host == "amp-api.music.apple.com",
      headers.isEmpty || url.host == "amp-api.music.apple.com", maximum > 0
    else { throw Self.invalidResponse() }
    var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData)
    request.setValue("Mozilla/5.0", forHTTPHeaderField: "User-Agent")
    for (key, value) in headers { request.setValue(value, forHTTPHeaderField: key) }
    let (bytes, response) = try await session.bytes(for: request, delegate: NoWebRedirects())
    defer { bytes.task.cancel() }
    guard let response = response as? HTTPURLResponse else { throw Self.invalidResponse() }
    guard response.statusCode == 200 else { throw WebHTTPFailure(status: response.statusCode) }
    return try await boundedBody(
      bytes, response: response, maximum: maximum, failure: Self.invalidResponse())
  }

  private func fetchToken() async throws -> Token {
    let page = try await load(URL(string: "https://music.apple.com/us/new")!, maximum: 4 << 20)
    guard let html = String(data: page, encoding: .utf8) else { throw Self.invalidResponse() }
    if let token = Self.extractToken(html, now: Date()) { return token }
    for url in Self.assets(html) {
      do {
        let data = try await load(url, maximum: 16 << 20)
        if let text = String(data: data, encoding: .utf8),
          let token = Self.extractToken(text, now: Date()) { return token }
      } catch {
        try Task.checkCancellation()
        if closed { throw CancellationError() }
        // A removed asset may coexist with a usable legacy asset; try at most two.
      }
    }
    throw Self.invalidResponse()
  }

  static func assets(_ html: String) -> [URL] {
    let regex = try! NSRegularExpression(pattern: #"src=["'](/assets/index[A-Za-z0-9_.~-]*\.js)["']"#)
    var result: [URL] = []
    regex.enumerateMatches(in: html, range: NSRange(html.startIndex..., in: html)) { match, _, stop in
      if let match, let range = Range(match.range(at: 1), in: html),
        let url = URL(string: "https://music.apple.com" + html[range]), !result.contains(url) {
        result.append(url)
        if result.count == 2 { stop.pointee = true }
      }
    }
    return result
  }

  static func extractToken(_ text: String, now: Date) -> Token? {
    struct Header: Decodable { let alg: String }
    struct Claims: Decodable { let iss: String; let exp: Double }
    let regex = try! NSRegularExpression(
      pattern: #"eyJ[A-Za-z0-9_-]{1,2048}\.eyJ[A-Za-z0-9_-]{1,8192}\.[A-Za-z0-9_-]{1,2048}"#)
    var result: Token?
    regex.enumerateMatches(in: text, range: NSRange(text.startIndex..., in: text)) { match, _, stop in
      guard let match, let range = Range(match.range, in: text) else { return }
      let value = String(text[range])
      let parts = value.split(separator: ".")
      func decode<T: Decodable>(_ type: T.Type, _ part: Substring) -> T? {
        let base = part.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        guard let data = Data(base64Encoded: base + String(repeating: "=", count: (4 - base.count % 4) % 4))
        else { return nil }
        return try? JSONDecoder().decode(type, from: data)
      }
      guard decode(Header.self, parts[0])?.alg == "ES256",
        let claims = decode(Claims.self, parts[1]), claims.iss == "AMPWebPlay", claims.exp.isFinite,
        claims.exp > now.timeIntervalSince1970 + 60
      else { return }
      result = Token(value: value, validUntil: Date(timeIntervalSince1970: claims.exp - 60))
      stop.pointee = true
    }
    return result
  }

  nonisolated static func invalidResponse() -> Failure {
    .init(code: .unavailable, message: "Apple Music 웹 응답을 읽을 수 없습니다. 다시 시도해주세요", retryable: true)
  }
}
