import CoreGraphics
import Foundation
import ImageIO
import MusicContract
import Testing

@testable import Backend

private func imageBytes() throws -> Data {
  let pixels = Data([255, 0, 0, 255, 0, 0, 255, 255])
  let provider = try #require(CGDataProvider(data: pixels as CFData))
  let image = try #require(
    CGImage(
      width: 1, height: 2, bitsPerComponent: 8, bitsPerPixel: 32,
      bytesPerRow: 4, space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: .init(rawValue: CGImageAlphaInfo.last.rawValue),
      provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent))
  let bytes = NSMutableData()
  let destination = try #require(
    CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil))
  CGImageDestinationAddImage(destination, image, nil)
  #expect(CGImageDestinationFinalize(destination))
  return bytes as Data
}

@MainActor @Test func artworkIsBoundedRGBInTopToBottomOrder() throws {
  let bytes = try imageBytes()
  let reference = ItemRef(id: "art", source: .catalog, kind: .album)
  let artwork = try ArtworkService.decode(bytes as Data, reference: reference)
  #expect(artwork.width <= 96 && artwork.height <= 96)
  #expect(artwork.rgb.count == Int(artwork.width * artwork.height * 3))
  #expect(Array(artwork.rgb.prefix(3)) == [255, 0, 0])
  #expect(Array(artwork.rgb.suffix(3)) == [0, 0, 255])
  #expect(throws: Failure.self) {
    try ArtworkService.decode(Data("invalid".utf8), reference: reference)
  }
}

private final class RequestCount: @unchecked Sendable {
  private let lock = NSLock()
  private var value = 0
  func increment() { lock.withLock { value += 1 } }
  var count: Int { lock.withLock { value } }
}

private final class ArtworkStub: URLProtocol, @unchecked Sendable {
  static let requests = RequestCount()
  override class func canInit(with request: URLRequest) -> Bool { true }
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
  override func startLoading() {
    Self.requests.increment()
    do {
      let data = try imageBytes()
      let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
      client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: data)
      client?.urlProtocolDidFinishLoading(self)
    } catch { client?.urlProtocol(self, didFailWithError: error) }
  }
  override func stopLoading() {}
}

@MainActor @Test func artworkFetchSupportsNativeURLsAndKeepsTheCacheBounded() async throws {
  let configuration = URLSessionConfiguration.ephemeral
  configuration.protocolClasses = [ArtworkStub.self]
  let session = URLSession(configuration: configuration)
  defer { session.invalidateAndCancel() }
  let artwork = ArtworkService(session: session)
  func item(_ index: Int) -> Item {
    var item = sampleSong(String(index))
    item.artworkUrl = "https://example.test/\(index).png"
    return item
  }
  for index in 0..<64 { _ = try await artwork.get(item(index)) }
  _ = try await artwork.get(item(0))
  _ = try await artwork.get(item(64))
  #expect(ArtworkStub.requests.count == 65)
  for index in [0, 63, 64] {
    #expect(try await artwork.get(item(index)).item == item(index).ref)
  }
  #expect(ArtworkStub.requests.count == 65)
  _ = try await artwork.get(item(1))
  #expect(ArtworkStub.requests.count == 66)

  var native = item(65)
  native.artworkUrl = "musicKit://artwork/cover"
  let image = try await artwork.get(native)
  #expect(image.item == native.ref && image.rgb.count == 6)
  for address in ["http://example.test/cover.png", "file:///tmp/cover.png", "other://artwork/cover"] {
    native.artworkUrl = address
    await #expect(throws: Failure.self) { try await artwork.get(native) }
  }
  #expect(ArtworkStub.requests.count == 67)
}
