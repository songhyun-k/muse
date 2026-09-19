import Foundation
import Testing

@testable import MusicContract

private func fixtures() throws -> [String: [[String: Any]]] {
  let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
  let data = try Data(contentsOf: root.appendingPathComponent("contract/fixtures.json"))
  return try #require(JSONSerialization.jsonObject(with: data) as? [String: [[String: Any]]])
}

@Test func sharedFixturesRejectUnknownAndMalformedMessages() throws {
  for (group, examples) in try fixtures() {
    for example in examples {
      let data = try JSONSerialization.data(withJSONObject: example)
      switch group {
      case "validRequests":
        let request = try Wire.decodeRequest(data)
        #expect(try Wire.decodeRequest(Wire.encode(request)) == request)
      case "validEvents":
        let event = try Wire.decodeEvent(data)
        #expect(try Wire.decodeEvent(Wire.encode(event)) == event)
      case "invalidEvents":
        #expect(throws: (any Error).self) { try Wire.decodeEvent(data) }
      default:
        #expect(throws: (any Error).self) { try Wire.decodeRequest(data) }
      }
    }
  }
}

@Test func resourceLimitsAndNonFiniteNumbersAreRejected() throws {
  #expect(throws: ContractError.tooLarge) {
    try Wire.decodeRequest(Data(repeating: 32, count: maxMessageBytes + 1))
  }
  let giant = Request(
    version: 1, id: 1,
    command: .search(
      .init(
        query: String(repeating: "가", count: 8193), source: .catalog, kind: .song, offset: 0)))
  #expect(throws: (any Error).self) { try Wire.encode(giant) }
  let nan = Request(version: 1, id: 1, command: .seek(.init(seconds: .nan, entryId: "entry")))
  #expect(throws: (any Error).self) { try Wire.encode(nan) }
  let nested = Data(
    (String(repeating: "[", count: 25) + "0" + String(repeating: "]", count: 25)).utf8)
  #expect(throws: (any Error).self) { try Wire.decodeRequest(nested) }
}

@Test func mutationBoundsFailBeforeReachingServices() throws {
  let invalid: [Command] = [
    .seek(.init(seconds: -1, entryId: "entry")), .volume(.init(level: 1.1)), .volume(.init()),
    .seek(.init(seconds: 10, entryId: "")),
    .play(
      .init(
        items: [.init(id: "song", source: .catalog, kind: .song)], startIndex: 0, placement: .last,
        shuffle: true)),
    .collectionCreate(.init(name: "  ", description: "")),
    .collectionUpdate(.init(id: "unchanged")),
    .cancel(.init(requestId: 1)),
    .play(.init(items: [], startIndex: 0, placement: .replace)),
  ]
  for command in invalid {
    let data = try Wire.encode(Request(version: 1, id: 1, command: command))
    #expect(throws: (any Error).self) { try Wire.decodeRequest(data) }
  }
}
