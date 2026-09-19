import Foundation
import MusicContract

// Consume buffered bytes off the main actor so each byte avoids an executor hop.
func boundedBody(
  _ bytes: URLSession.AsyncBytes, response: URLResponse, maximum: Int, failure: Failure
) async throws -> Data {
  guard response.expectedContentLength <= maximum else { throw failure }
  var data = Data()
  for try await byte in bytes {
    guard data.count < maximum else { throw failure }
    data.append(byte)
  }
  try Task.checkCancellation()
  return data
}
