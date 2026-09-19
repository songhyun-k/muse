import Foundation
import MusicContract
import Testing

@testable import Backend

@MainActor @Test func savedCollectionJourneySurvivesARealServiceRelaunch() async throws {
  let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  let file = directory.appendingPathComponent("library.json")
  defer { try? FileManager.default.removeItem(at: directory) }
  let song = sampleSong("saved")
  do {
    let seed = try LibraryStore(file: file)
    try seed.favorite(song, enabled: true)
  }
  var service: Service? = Service(storeURL: file)
  var requestID: UInt64 = 0
  func send(_ command: Command) async throws -> Notice {
    requestID += 1
    return await service!.handle(
      try Wire.encode(Request(version: apiVersion, id: requestID, command: command))
    ).event
  }
  let created = try await send(
    .collectionCreate(.init(name: "밤", description: "기록", items: [song.ref])))
  guard case .collectionCreated(let value) = created else {
    Issue.record("create failed")
    return
  }
  let id = value.collection.id
  _ = try await send(.collectionUpdate(.init(id: id, name: "밤 산책")))
  _ = try await send(.collectionAdd(.init(id: id, items: [song.ref])))
  _ = try await send(.collectionRemove(.init(id: id, index: 0)))
  _ = try await send(.favorite(.init(item: song.ref, enabled: false)))
  service?.close()
  service = nil
  service = Service(storeURL: file)
  defer { service?.close() }
  let result = try await send(
    .detail(.init(item: .init(id: id, source: .collection, kind: .playlist), offset: 0)))
  guard case .detail(let detail) = result else {
    Issue.record("reload failed")
    return
  }
  #expect(detail.item.title == "밤 산책" && detail.children == [song])
  let store = try service!.store.get()
  #expect(store.summary.collections[0].id == id)
  #expect(store.summary.collections[0].description == "기록")
  #expect(store.summary.favorites.isEmpty)
  _ = try await send(.collectionDelete(.init(id: id)))
  #expect(store.summary.collections.isEmpty)
}
