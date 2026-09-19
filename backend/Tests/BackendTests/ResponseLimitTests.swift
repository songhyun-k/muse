import Foundation
import MusicContract
import Testing

@testable import Backend

private final class OversizedResponseStub: URLProtocol, @unchecked Sendable {
  static let stopped = AsyncStream<URL>.makeStream()

  override class func canInit(with request: URLRequest) -> Bool { true }
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
  override func startLoading() {
    let response = HTTPURLResponse(
      url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
    client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
    let maximum = request.url?.host == "lrclib.net" ? 1 << 20 : 8 << 20
    client?.urlProtocol(self, didLoad: Data(count: maximum + 1))
    // Deliberately omit Content-Length and EOF: the consumer must stop this stream.
  }
  override func stopLoading() { Self.stopped.continuation.yield(request.url!) }
}

@MainActor @Test(.timeLimit(.minutes(1)))
func oversizedArtworkAndLyricsCancelBeforeEOF() async throws {
  let configuration = URLSessionConfiguration.ephemeral
  configuration.protocolClasses = [OversizedResponseStub.self]
  let session = URLSession(configuration: configuration)
  defer { session.invalidateAndCancel() }
  var item = sampleSong("oversized")
  item.artworkUrl = "https://example.test/cover.png"
  let fetches: [@MainActor () async throws -> Void] = [
    { _ = try await ArtworkService(session: session).get(item) },
    { _ = try await LyricsService(session: session).load(item) },
  ]
  var stopped = OversizedResponseStub.stopped.stream.makeAsyncIterator()
  for fetch in fetches {
    do {
      try await fetch()
      Issue.record("Oversized stream was accepted")
    } catch let failure as Failure {
      #expect(failure.code == .network && failure.retryable)
    }
    #expect(await stopped.next() != nil)
  }
}
