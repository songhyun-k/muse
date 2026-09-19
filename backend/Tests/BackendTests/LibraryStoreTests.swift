import Foundation
import MusicContract
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

@MainActor @Test func serviceRetriesInitialStoreLockAndKeepsTheRecoveredStore() async throws {
  let file = try temporaryFile()
  defer { try? FileManager.default.removeItem(at: file.deletingLastPathComponent()) }
  var owner: LibraryStore? = try LibraryStore(file: file)
  let song = sampleSong("saved")
  try owner?.favorite(song, enabled: true)
  let service = Service(storeURL: file, music: MusicService(authorization: { .denied }))
  defer { service.close() }
  let browse = Command.browse(.init(scope: .favorites, kind: .song, order: .recent, offset: 0))
  let create = Command.collectionCreate(.init(name: "Recovered", description: ""))
  var requestID: UInt64 = 0
  func send(_ command: Command) async throws -> Notice {
    requestID += 1
    return await service.handle(
      try Wire.encode(Request(version: apiVersion, id: requestID, command: command))
    ).event
  }
  let original = try Data(contentsOf: file)
  for command in [browse, create] {
    guard case .failure(let failure) = try await send(command) else {
      Issue.record("Expected a locked store failure")
      return
    }
    #expect(failure.code == .conflict && failure.retryable)
  }
  #expect(try Data(contentsOf: file) == original)
  owner = nil
  guard case .snapshot(let snapshot) = try await send(.snapshot(.init())),
    case .page(let page) = try await send(browse)
  else {
    Issue.record("Expected the same service to recover after the lock is released")
    return
  }
  #expect(snapshot.store.favorites == [song.ref] && page.items == [song])
  let recovered = try service.store.get()
  #expect(throws: StoreError.locked) { try LibraryStore(file: file) }
  guard case .collectionCreated(let created) = try await send(create) else {
    Issue.record("Expected a successful write after recovery")
    return
  }
  #expect(try service.store.get() === recovered)
  #expect(recovered.summary.collections == [created.collection])
  let persisted = try JSONDecoder().decode(SavedLibrary.self, from: Data(contentsOf: file))
  #expect(persisted.favorites == [song] && persisted.collections.map(\.summary) == [created.collection])
}

@MainActor @Test func corruptStorageIsNotOverwritten() async throws {
  let file = try temporaryFile()
  defer { try? FileManager.default.removeItem(at: file.deletingLastPathComponent()) }
  let corrupt = Data("{broken".utf8)
  try corrupt.write(to: file)
  #expect(throws: StoreError.read) { try LibraryStore(file: file) }
  let service = Service(storeURL: file)
  defer { service.close() }
  for id: UInt64 in 1...2 {
    let response = await service.handle(try Wire.encode(Request(
      version: apiVersion, id: id,
      command: .collectionCreate(.init(name: "Must not overwrite", description: "")))))
    guard case .failure(let failure) = response.event else {
      Issue.record("Expected corrupt storage to reject the write")
      return
    }
    #expect(failure.code == .storage)
  }
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
