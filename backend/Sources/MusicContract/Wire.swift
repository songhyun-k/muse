import Foundation

extension Failure: Error {}

public enum ContractError: Error, Equatable {
  case tooLarge
  case invalid(String)
  case unsupportedVersion
}

public enum Wire {
  /// Recover correlation only from a bounded, syntactically valid envelope.
  public static func requestID(in data: Data) -> UInt64? {
    struct Envelope: Decodable { let id: UInt64 }
    guard (try? checkBudget(data)) != nil,
      let envelope = try? JSONDecoder().decode(Envelope.self, from: data), envelope.id > 0
    else { return nil }
    return envelope.id
  }

  public static func decodeRequest(_ data: Data) throws -> Request {
    try checkBudget(data)
    let request = try JSONDecoder().decode(Request.self, from: data)
    guard request.version == apiVersion else { throw ContractError.unsupportedVersion }
    try require(request.id > 0, "Request ID must be nonzero")
    try validate(request)
    return request
  }

  public static func decodeEvent(_ data: Data) throws -> Event {
    try checkBudget(data)
    let event = try JSONDecoder().decode(Event.self, from: data)
    guard event.version == apiVersion else { throw ContractError.unsupportedVersion }
    try require(event.sequence > 0 && event.id != 0, "Invalid event identity")
    return event
  }

  public static func encode<T: Encodable>(_ value: T) throws -> Data {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
    let data = try encoder.encode(value)
    try checkBudget(data)
    return data
  }

  private static func checkBudget(_ data: Data) throws {
    guard data.count <= maxMessageBytes else { throw ContractError.tooLarge }
    let value: Any
    do { value = try JSONSerialization.jsonObject(with: data) }
    catch { throw ContractError.invalid("Invalid JSON") }
    try walk(value, depth: 0)
  }

  private static func walk(_ value: Any, depth: Int) throws {
    try require(depth <= 24, "Message is too deeply nested")
    switch value {
    case let text as String:
      try require(text.unicodeScalars.count <= 8192, "String is too long")
    case let array as [Any]:
      try require(array.count <= 32768, "Array is too large")
      for element in array { try walk(element, depth: depth + 1) }
    case let object as [String: Any]:
      for (key, element) in object {
        try walk(key, depth: depth + 1)
        try walk(element, depth: depth + 1)
      }
    default: break
    }
  }

  private static func require(_ condition: Bool, _ message: String) throws {
    guard condition else { throw ContractError.invalid(message) }
  }

  private static func identifier(_ value: String) throws {
    try require(!value.isEmpty && value.unicodeScalars.count <= 256, "Invalid identifier")
    try require(
      !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
      "Control character in identifier")
  }

  private static func reference(_ item: ItemRef) throws {
    try identifier(item.id)
    try require(
      item.source != .collection || item.kind == .playlist, "Invalid collection reference")
  }

  private static func name(_ value: String, description: String) throws {
    try require(
      !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        && value.unicodeScalars.count <= 100, "Invalid collection name")
    try require(description.unicodeScalars.count <= 2000, "Description is too long")
  }

  private static func references(_ items: [ItemRef]) throws {
    try require(!items.isEmpty && items.count <= 2000, "Expected 1...2000 items")
    for item in items { try reference(item) }
  }

  private static func validate(_ request: Request) throws {
    switch request.command {
    case .snapshot, .authorize, .control, .mode, .queueClear: break
    case .queue(let params):
      try require(params.offset <= 1_000_000, "Invalid queue offset")
    case .search(let params):
      try require(
        !params.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          && params.query.unicodeScalars.count <= 200, "Invalid search query")
      try require(
        params.source != .collection && params.offset <= 1_000_000, "Invalid search scope")
    case .browse(let params):
      try require(params.offset <= 1_000_000, "Invalid offset")
      if params.scope == .collection {
        guard let id = params.collectionId else {
          throw ContractError.invalid("Collection ID required")
        }
        try identifier(id)
      }
    case .detail(let params):
      try reference(params.item)
      try require(params.offset <= 1_000_000, "Invalid offset")
    case .play(let params):
      try references(params.items)
      try require(params.startIndex < params.items.count, "Invalid start index")
      try require(
        params.shuffle == nil || params.placement == .replace, "Shuffle requires replacement")
    case .seek(let params):
      try identifier(params.entryId)
      try require(params.seconds.isFinite && params.seconds >= 0, "Invalid seek time")
    case .queueRemove(let params): try identifier(params.entryId)
    case .queueJump(let params): try identifier(params.entryId)
    case .queueMove(let params):
      try identifier(params.entryId)
      if let before = params.beforeEntryId { try identifier(before) }
    case .collectionCreate(let params):
      try name(params.name, description: params.description)
      if let items = params.items, !items.isEmpty {
        try references(items)
        try require(items.allSatisfy { $0.kind == .song }, "Expected songs")
      }
    case .collectionUpdate(let params):
      try identifier(params.id)
      try require(params.name != nil || params.description != nil, "Expected a changed field")
      if let value = params.name { try name(value, description: "") }
      if let value = params.description {
        try require(value.unicodeScalars.count <= 2000, "Description is too long")
      }
    case .collectionDelete(let params): try identifier(params.id)
    case .collectionAdd(let params):
      try identifier(params.id)
      try references(params.items)
    case .collectionRemove(let params):
      try identifier(params.id)
      try require(params.index < 2000, "Invalid collection index")
    case .collectionMove(let params):
      try identifier(params.id)
      try require(params.from < 2000 && params.to < 2000, "Invalid collection index")
    case .collectionCopy(let params):
      try reference(params.item)
      try name(params.name, description: "")
    case .favorite(let params): try reference(params.item)
    case .lyrics(let params): try reference(params.item)
    case .lyricsChoose(let params):
      try reference(params.item)
      try require(params.matchId > 0, "Invalid lyric match")
    case .lyricsSearch(let params):
      try reference(params.item)
      try require(
        !params.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          && params.query.unicodeScalars.count <= 200, "Invalid lyric search")
    case .lyricsOffset(let params):
      try reference(params.item)
      try require(
        params.seconds.isFinite && (-30...30).contains(params.seconds), "Invalid lyric offset")
    case .volume(let params):
      try require(params.level != nil || params.muted != nil, "Volume change required")
      if let level = params.level {
        try require(level.isFinite && (0...1).contains(level), "Invalid volume")
      }
    case .artwork(let params): try reference(params.item)
    case .cancel(let params):
      try require(
        params.requestId > 0 && params.requestId != request.id, "Invalid cancellation target")
    }
  }
}
