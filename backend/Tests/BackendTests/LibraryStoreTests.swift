import Foundation
import Testing

@testable import Backend

private func temporaryFile() throws -> URL {
  let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  return directory.appendingPathComponent("library.json")
}

@MainActor @Test func persistenceRoundTripAndExclusiveWriter() throws {
  let file = try temporaryFile()
  defer { try? FileManager.default.removeItem(at: file.deletingLastPathComponent()) }
  var store: LibraryStore? = try LibraryStore(file: file)
  try store?.transaction { library in
    library.collections.append(.init(id: "stable-id", name: "Evening", description: "밤의 음악"))
  }
  #expect(throws: StoreError.locked) { try LibraryStore(file: file) }
  let expected = try #require(store?.state)
  store = nil
  let reopened = try LibraryStore(file: file)
  #expect(reopened.state == expected)
  #expect(reopened.state.collections.first?.id == "stable-id")
}

@MainActor @Test func corruptStorageIsNotOverwritten() throws {
  let file = try temporaryFile()
  defer { try? FileManager.default.removeItem(at: file.deletingLastPathComponent()) }
  let corrupt = Data("{broken".utf8)
  try corrupt.write(to: file)
  #expect(throws: StoreError.read) { try LibraryStore(file: file) }
  #expect(try Data(contentsOf: file) == corrupt)
}

@MainActor @Test func failedWritePreservesMemoryAndPreviousFile() throws {
  let file = try temporaryFile()
  let directory = file.deletingLastPathComponent()
  defer {
    try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
    try? FileManager.default.removeItem(at: directory)
  }
  let store = try LibraryStore(file: file)
  try store.transaction { $0.collections.append(.init(id: "one", name: "Before", description: "")) }
  let previous = try Data(contentsOf: file)
  try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: directory.path)
  #expect(throws: StoreError.write) {
    try store.transaction { $0.collections[0].name = "After" }
  }
  #expect(store.state.collections[0].name == "Before")
  #expect(try Data(contentsOf: file) == previous)
}

@MainActor @Test func invalidMutationRollsBack() throws {
  let store = try LibraryStore()
  #expect(throws: StoreError.invalidData) {
    try store.transaction { $0.collections.append(.init(id: "one", name: " ", description: "")) }
  }
  #expect(store.state.collections.isEmpty)
}
