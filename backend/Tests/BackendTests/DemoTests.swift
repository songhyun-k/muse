import Foundation
import MusicContract
import Testing

@testable import Backend

@Test func embeddedDemoMatchesTheApprovedCorpusWithoutExternalFiles() throws {
  let catalog = try DemoCatalog.load()
  #expect(catalog.tracks.count == 16 && catalog.albums.count == 6)
  #expect(catalog.seed.collections.first?.id == "evening")
  #expect(catalog.seed.collections.first?.items.count == 8)
  #expect(catalog.player.current?.title == "Soft Signal" && catalog.player.position == 12)
  let image = try catalog.artwork(catalog.tracks[0].ref)
  #expect(image.rgb.count == 27648)
  #expect(try catalog.lyric(catalog.tracks[0].ref).lines.count == 12)
  let results = try catalog.search(.init(query: "유나", source: .catalog, kind: .song, offset: 0))
  #expect(results.items.count == 4)
  #expect(results.items.allSatisfy { $0.ref.source == .catalog && $0.albumRef?.source == .catalog })
  let detail = try catalog.detail(.init(item: catalog.tracks[0].ref, offset: 0))
  #expect(detail.item.ref.kind == .album && detail.children.count == 1)
  #expect(try catalog.songs(catalog.albums[1].ref).count == 7)
  #expect(throws: Failure.self) {
    try catalog.item(.init(id: "unknown", source: .catalog, kind: .song))
  }
}

@MainActor @Test func demoCommandsShareTheContractAndDomainRulesWithoutNativePlayback() async throws
{
  let demo = try DemoService()
  defer { demo.close() }
  var id: UInt64 = 0
  func send(_ command: Command) async throws -> Notice {
    id += 1
    let request = Request(version: apiVersion, id: id, command: command)
    let bytes = try JSONEncoder().encode(request)
    let event = await withCheckedContinuation { continuation in
      demo.submit(bytes) { continuation.resume(returning: $0) }
    }
    #expect(event.id == id)
    _ = try Wire.encode(event)
    return event.event
  }
  let first = demo.catalog.tracks[0].ref
  _ = try await send(.control(.init(action: .pause)))
  let position = demo.player.position
  demo.advance(to: demo.player.updatedAt + 10)
  #expect(demo.player.position == position)
  _ = try await send(
    .play(.init(items: [first, first, first], startIndex: 0, placement: .replace)))
  #expect(demo.queue.count == 2 && demo.queue[0].id != demo.queue[1].id)
  let entry = try #require(demo.player.currentEntryId)
  _ = try await send(.seek(.init(seconds: 50, entryId: entry)))
  #expect(demo.player.position == 50)
  _ = try await send(.control(.init(action: .next)))
  let stale = try await send(.seek(.init(seconds: 10, entryId: entry)))
  guard case .failure(let failure) = stale else {
    Issue.record("stale seek succeeded")
    return
  }
  #expect(failure.code == .conflict && demo.player.position < 1)
  let before = demo.queue
  let badMove = try await send(.queueMove(.init(entryId: before[0].id, beforeEntryId: "gone")))
  guard case .failure = badMove else {
    Issue.record("invalid queue move succeeded")
    return
  }
  #expect(demo.queue == before)
  _ = try await send(.volume(.init(level: 0.25, muted: true)))
  #expect(demo.volume.level == 0.25 && demo.volume.muted == true)
  let created = try await send(
    .collectionCreate(.init(name: "새 목록", description: "설명", items: [first])))
  guard case .collectionCreated(let result) = created else {
    Issue.record("creation failed")
    return
  }
  _ = try await send(.collectionUpdate(.init(id: result.collection.id, name: "수정")))
  let collection = try demo.collection(result.collection.id)
  #expect(collection.name == "수정" && collection.description == "설명" && collection.items.count == 1)
  _ = try await send(.lyricsChoose(.init(item: first, matchId: 1)))
  _ = try await send(.lyricsOffset(.init(item: first, seconds: 1.25)))
  let lyric = try await send(.lyrics(.init(item: first)))
  guard case .lyrics(let value) = lyric else {
    Issue.record("lyrics failed")
    return
  }
  #expect(value.offset == 1.25 && value.matchId == 1 && value.item == first)
  let secondSession = try DemoService()
  defer { secondSession.close() }
  #expect(secondSession.store.summary.collections.count == 1)
  #expect(secondSession.volume.level == 0.65)
}
