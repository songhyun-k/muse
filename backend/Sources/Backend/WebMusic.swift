import Foundation
import MusicContract
@preconcurrency import MusicKit

struct WebPage<Element: Decodable>: Decodable {
  let data: [Element]
  let next: String?

  func nextOffset(after offset: UInt64) throws -> UInt64? {
    guard let next else { return nil }
    guard !data.isEmpty,
      let value = URLComponents(string: next)?.queryItems?.first(where: { $0.name == "offset" })?.value,
      let nextOffset = UInt64(value), nextOffset > offset, nextOffset <= 1_000_000
    else { throw AppleWeb.invalidResponse() }
    return nextOffset
  }
}

private struct WebSearch<Element: Decodable>: Decodable {
  let results: [String: WebPage<Element>]
}

@MainActor
final class WebMusic {
  typealias UserToken = @MainActor (String, Bool) async throws -> String
  private let http: AppleWeb
  private let userToken: UserToken
  private var storefront: (user: String, country: String)?

  init(http: AppleWeb = AppleWeb(), userToken: @escaping UserToken = { token, refresh in
    try await MusicUserTokenProvider().userToken(for: token, options: refresh ? .ignoreCache : [])
  }) {
    self.http = http
    self.userToken = userToken
  }

  func close() { storefront = nil; http.close() }

  func checkLogin() async throws {
    let token = try await http.token()
    let user = try await userToken(token.value, true)
    try Task.checkCancellation()
    guard !user.isEmpty else { throw MusicTokenRequestError.userNotSignedIn }
  }

  func search<T: Decodable>(_ params: SearchParams) async throws -> WebPage<T> {
    let resource = params.kind.rawValue + "s"
    let response: WebSearch<T> = try await read("/search", query: [
      .init(name: "term", value: params.query.trimmingCharacters(in: .whitespacesAndNewlines)),
      .init(name: "types", value: resource), .init(name: "limit", value: "25"),
      .init(name: "offset", value: String(params.offset))
    ])
    return response.results[resource] ?? WebPage(data: [], next: nil)
  }

  func resource<T: MusicItem & Decodable>(
    _ reference: ItemRef, include: [String] = []
  ) async throws -> T {
    var query = [URLQueryItem(name: "ids", value: reference.id)]
    if !include.isEmpty { query.append(.init(name: "include", value: include.joined(separator: ","))) }
    let response: WebPage<T> = try await read("/" + reference.kind.rawValue + "s", query: query)
    guard let item = response.data.first(where: { $0.id.rawValue == reference.id }) else {
      throw Failure(code: .notFound, message: "이 항목을 현재 지역에서 찾을 수 없습니다", retryable: false)
    }
    return item
  }

  func relationship<T: Decodable>(
    _ reference: ItemRef, name: String, offset: UInt64, limit: Int
  ) async throws -> WebPage<T> {
    guard reference.id != ".", reference.id != "..", !reference.id.isEmpty,
      ["tracks", "albums"].contains(name),
      let id = reference.id.addingPercentEncoding(
        withAllowedCharacters: .alphanumerics.union(CharacterSet(charactersIn: "-_.")))
    else { throw AppleWeb.invalidResponse() }
    return try await read("/\(reference.kind.rawValue)s/\(id)/\(name)", query: [
      .init(name: "offset", value: String(offset)), .init(name: "limit", value: String(limit))
    ])
  }

  func recommendations() async throws -> MusicPersonalRecommendationsResponse {
    try await read("/v1/me/recommendations", query: [.init(name: "limit", value: "10")], catalog: false)
  }

  private func read<T: Decodable>(
    _ path: String, query: [URLQueryItem], catalog: Bool = true
  ) async throws -> T {
    for refresh in [false, true] {
      let token = try await http.token()
      do {
        let user = try await userToken(token.value, refresh)
        try Task.checkCancellation()
        guard !user.isEmpty else { throw MusicTokenRequestError.userNotSignedIn }
        let headers = [
          "Authorization": "Bearer " + token.value,
          "Media-User-Token": user,
          "Origin": "https://music.apple.com", "Referer": "https://music.apple.com/"
        ]
        let prefix = catalog ? "/v1/catalog/" + (try await country(user: user, headers: headers)) : ""
        let data = try await fetch(prefix + path, query: query, headers: headers)
        do { return try JSONDecoder().decode(T.self, from: data) }
        catch { throw AppleWeb.invalidResponse() }
      } catch let error as WebHTTPFailure {
        guard !refresh, [401, 403].contains(error.status) else {
          throw RequestScheduler.musicHTTPFailure(status: error.status)
        }
      } catch let error as MusicTokenRequestError {
        guard !refresh, [.userTokenRequestFailed, .userTokenRevoked].contains(error) else { throw error }
      }
      // Refresh the public/user pair together; ignore late failures of older public tokens.
      http.invalidate(token.value)
      storefront = nil
    }
    throw AppleWeb.invalidResponse()
  }

  private func country(user: String, headers: [String: String]) async throws -> String {
    if let storefront, storefront.user == user { return storefront.country }
    struct Storefront: Decodable { let id: String }
    let data = try await fetch("/v1/me/storefront", query: [], headers: headers)
    guard let value = try? JSONDecoder().decode(WebPage<Storefront>.self, from: data).data.first?.id,
      value.utf8.count == 2, value.utf8.allSatisfy({ (97...122).contains($0) })
    else { throw AppleWeb.invalidResponse() }
    storefront = (user, value)
    return value
  }

  private func fetch(_ path: String, query: [URLQueryItem], headers: [String: String]) async throws -> Data {
    var url = URLComponents(string: "https://amp-api.music.apple.com")!
    url.percentEncodedPath = path
    url.queryItems = query.isEmpty ? nil : query
    guard let url = url.url else { throw AppleWeb.invalidResponse() }
    return try await http.load(url, headers: headers, maximum: 8 << 20)
  }
}
