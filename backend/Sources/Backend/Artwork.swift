import CoreGraphics
import Foundation
import ImageIO
import MusicContract

@MainActor
final class ArtworkService {
  private var cache: [String: MusicContract.Artwork] = [:]
  private var order: [String] = []
  private let session: URLSession

  init(session: URLSession = .shared) { self.session = session }

  private func touch(_ address: String) {
    // ponytail: linear order updates are bounded at 64 cached URLs.
    order.removeAll { $0 == address }
    order.append(address)
  }

  func get(_ item: Item) async throws -> MusicContract.Artwork {
    guard let address = item.artworkUrl, let url = URL(string: address),
      ["https", "musickit"].contains(url.scheme?.lowercased() ?? "")
    else {
      throw Failure(code: .notFound, message: "앨범 이미지가 없습니다", retryable: false)
    }
    if var cached = cache[address] {
      cached.item = item.ref
      touch(address)
      return cached
    }
    let (data, response) = try await session.data(
      for: URLRequest(url: url, timeoutInterval: 10))
    guard let response = response as? HTTPURLResponse, (200..<300).contains(response.statusCode),
      data.count <= 8 * 1024 * 1024
    else {
      throw Failure(code: .network, message: "앨범 이미지를 불러오지 못했습니다", retryable: true)
    }
    try Task.checkCancellation()
    let artwork = try Self.decode(data, reference: item.ref)
    if cache[address] == nil && cache.count >= 64 {
      cache.removeValue(forKey: order.removeFirst())
    }
    cache[address] = artwork
    touch(address)
    return artwork
  }

  static func decode(_ data: Data, reference: ItemRef) throws -> MusicContract.Artwork {
    let options: [CFString: Any] = [
      kCGImageSourceCreateThumbnailFromImageAlways: true,
      kCGImageSourceThumbnailMaxPixelSize: 96, kCGImageSourceCreateThumbnailWithTransform: true,
    ]
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
      let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary),
      image.width > 0, image.height > 0, image.width <= 96, image.height <= 96
    else {
      throw Failure(code: .unavailable, message: "앨범 이미지를 표시할 수 없습니다", retryable: false)
    }
    var rgba = [UInt8](repeating: 0, count: image.width * image.height * 4)
    let decoded = rgba.withUnsafeMutableBytes { bytes -> Bool in
      guard
        let context = CGContext(
          data: bytes.baseAddress, width: image.width, height: image.height,
          bitsPerComponent: 8, bytesPerRow: image.width * 4, space: CGColorSpaceCreateDeviceRGB(),
          bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue)
      else { return false }
      context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
      return true
    }
    guard decoded else {
      throw Failure(code: .unavailable, message: "앨범 이미지를 표시할 수 없습니다", retryable: false)
    }
    let rgb = stride(from: 0, to: rgba.count, by: 4).flatMap { Array(rgba[$0..<$0 + 3]) }
    return .init(
      item: reference, width: UInt64(image.width), height: UInt64(image.height), rgb: rgb)
  }
}
