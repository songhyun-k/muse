import Darwin
import Foundation
import MusicContract

enum StoreError: Error, Equatable {
  case locked, invalidData, read, write
}

struct SavedCollection: Codable, Equatable {
  var id: String
  var name: String
  var description: String
  var items: [Item] = []

  var summary: MusicContract.Collection {
    .init(id: id, name: name, description: description, count: UInt64(items.count))
  }
}

struct LyricPreference: Codable, Equatable {
  var matchId: UInt64?
  var offset: Double = 0
}

struct SavedLibrary: Codable, Equatable {
  var version = 1
  var collections: [SavedCollection] = []
  var favorites: [Item] = []
  var history: [Item] = []
  var lyrics: [String: LyricPreference] = [:]
}

extension ItemRef {
  var storageKey: String { "\(source.rawValue):\(kind.rawValue):\(id)" }
}

private final class StoreLock {
  let descriptor: Int32

  init(file: URL) throws {
    let opened = open(file.path, O_CREAT | O_RDWR | O_CLOEXEC, 0o600)
    guard opened >= 0 else { throw StoreError.write }
    guard flock(opened, LOCK_EX | LOCK_NB) == 0 else {
      close(opened)
      throw StoreError.locked
    }
    descriptor = opened
  }

  deinit { close(descriptor) }
}

@MainActor
final class LibraryStore {
  private(set) var state = SavedLibrary()
  private let file: URL?
  private let fileLock: StoreLock?

  init(file: URL? = nil) throws {
    self.file = file
    if let file {
      try FileManager.default.createDirectory(
        at: file.deletingLastPathComponent(),
        withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
      fileLock = try StoreLock(file: file.appendingPathExtension("lock"))
      if FileManager.default.fileExists(atPath: file.path) {
        do {
          let bytes = try Data(contentsOf: file, options: .mappedIfSafe)
          guard bytes.count <= 64 * 1024 * 1024 else { throw StoreError.invalidData }
          state = try JSONDecoder().decode(SavedLibrary.self, from: bytes)
          try Self.validate(state)
        } catch {
          throw StoreError.read
        }
      }
    } else {
      fileLock = nil
    }
  }

  func transaction(_ edit: (inout SavedLibrary) throws -> Void) throws {
    var candidate = state
    try edit(&candidate)
    try Self.validate(candidate)
    if let file {
      do {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(candidate)
        guard data.count <= 64 * 1024 * 1024 else { throw StoreError.invalidData }
        // ponytail: whole-document atomic writes; use SQLite if the bounded library
        // grows enough for measured write latency to affect interaction.
        try data.write(to: file, options: .atomic)
      } catch {
        throw StoreError.write
      }
    }
    state = candidate
  }

  private static func validate(_ state: SavedLibrary) throws {
    guard state.version == 1, state.collections.count <= 100,
      Set(state.collections.map(\.id)).count == state.collections.count,
      state.favorites.count <= 2000, state.history.count <= 1000,
      Set(state.favorites.map { $0.ref.storageKey }).count == state.favorites.count,
      state.lyrics.count <= 5000
    else { throw StoreError.invalidData }
    for collection in state.collections {
      guard !collection.id.isEmpty,
        !collection.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        collection.name.unicodeScalars.count <= 100,
        collection.description.unicodeScalars.count <= 2000, collection.items.count <= 2000
      else { throw StoreError.invalidData }
    }
    let items = state.collections.flatMap(\.items) + state.favorites + state.history
    for item in items {
      guard !item.ref.id.isEmpty, item.ref.id.unicodeScalars.count <= 256,
        item.ref.source != .collection, item.ref.kind == .song,
        [item.title, item.artist, item.album].allSatisfy({ $0.unicodeScalars.count <= 8192 }),
        item.duration.map({ $0.isFinite && $0 >= 0 }) ?? true
      else { throw StoreError.invalidData }
    }
    for preference in state.lyrics.values {
      guard preference.offset.isFinite && (-30...30).contains(preference.offset),
        preference.matchId != 0
      else { throw StoreError.invalidData }
    }
  }
}
