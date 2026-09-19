import Foundation
import MusicContract
import Testing

@testable import Backend

func sampleSong(_ id: String, source: Source = .catalog) -> Item {
  .init(
    ref: .init(id: id, source: source, kind: .song), title: "곡 \(id)", artist: "가수",
    album: "앨범", duration: 200)
}

@MainActor @Test func collectionEditingPreservesDuplicatesAndIdentity() throws {
  let store = try LibraryStore()
  let song = sampleSong("1")
  let other = sampleSong("2")
  let summary = try store.create(name: " Evening ", description: "음악", items: [song, other, song])
  let id = summary.collection.id
  try store.update(.init(id: id, name: "Night", description: "바뀐 설명"))
  try store.move(.init(id: id, from: 1, to: 2))
  #expect(store.state.collections[0].items == [song, song, other])
  try store.remove(.init(id: id, index: 0))
  #expect(store.state.collections[0].items == [song, other])
  #expect(store.summary.collections[0].id == id)
  #expect(store.summary.collections[0].count == 2)
  let before = store.state
  #expect(throws: Failure.self) { try store.move(.init(id: id, from: 0, to: 999)) }
  #expect(store.state == before)
  try store.delete(id)
  #expect(store.state.collections.isEmpty)
}

@MainActor @Test func favoritesAreIdempotentAndSourceSpecific() throws {
  let store = try LibraryStore()
  try store.favorite(sampleSong("1"), enabled: true)
  try store.favorite(sampleSong("1"), enabled: true)
  try store.favorite(sampleSong("1", source: .library), enabled: true)
  #expect(store.summary.favorites.count == 2)
  try store.favorite(sampleSong("1"), enabled: false)
  #expect(store.summary.favorites == [sampleSong("1", source: .library).ref])
}

@MainActor @Test func partialCollectionEditsPreserveOtherFields() throws {
  let store = try LibraryStore()
  let created = try store.create(name: "First", description: "Original")
  let id = created.collection.id
  try store.update(.init(id: id, name: "Renamed"))
  try store.update(.init(id: id, description: "New description"))
  #expect(store.summary.collections[0].name == "Renamed")
  #expect(store.summary.collections[0].description == "New description")
  try store.update(.init(id: id, description: ""))
  #expect(store.summary.collections[0].name == "Renamed")
  #expect(store.summary.collections[0].description.isEmpty)
}

@MainActor @Test(arguments: [Order.recent, .name, .artist])
func localPaginationPreservesCollectionOrder(order: Order) throws {
  let store = try LibraryStore()
  let items = (0..<53).map { sampleSong(String($0)) }
  let result = try store.create(name: "List", description: "", items: items)
  let id = result.collection.id
  let first = try #require(
    try store.localPage(
      .init(
        scope: .collection, kind: .song,
        collectionId: id, order: order, offset: 0)))
  #expect(first.items == Array(items.prefix(50)))
  #expect(first.nextOffset == 50)
  let last = try #require(
    try store.localPage(
      .init(
        scope: .collection, kind: .song,
        collectionId: id, order: order, offset: 50)))
  #expect(last.items == Array(items.suffix(3)))
  #expect(last.nextOffset == nil)
}

@MainActor @Test func localOrderingPrecedesPagination() throws {
  let store = try LibraryStore()
  let songs = (0..<53).map { index in
    var song = sampleSong(String(index))
    song.artist = "Artist \(52 - index)"
    return song
  }
  for song in songs {
    try store.create(name: song.title, description: "")
    try store.favorite(song, enabled: true)
    try store.recordPlayback(song)
  }
  let before = store.state
  let newestFirst = Array(songs.reversed())
  for scope in [Scope.collections, .favorites, .history] {
    let kind: Kind = scope == .collections ? .playlist : .song
    let first = try #require(
      try store.localPage(.init(scope: scope, kind: kind, order: .recent, offset: 0)))
    #expect(first.items.map(\.title) == newestFirst.prefix(50).map(\.title))
    #expect(first.nextOffset == 50)
    let last = try #require(
      try store.localPage(.init(scope: scope, kind: kind, order: .recent, offset: 50)))
    #expect(last.items.map(\.title) == newestFirst.suffix(3).map(\.title))
    #expect(last.nextOffset == nil)
    let named = try #require(
      try store.localPage(.init(scope: scope, kind: kind, order: .name, offset: 0)))
    #expect(named.items.map(\.title) == songs.prefix(50).map(\.title))
    if scope != .collections {
      let artists = try #require(
        try store.localPage(.init(scope: scope, kind: kind, order: .artist, offset: 0)))
      #expect(artists.items == Array(newestFirst.prefix(50)))
    }
  }
  #expect(store.state == before)
}

@MainActor @Test func collectionRequestsReturnConfirmedState() async throws {
  let service = Service()
  let request = Request(
    version: 1, id: 42, command: .collectionCreate(.init(name: "Night", description: "")))
  let response = await service.handle(try Wire.encode(request))
  #expect(response.id == 42)
  guard case .collectionCreated(let state) = response.event else {
    Issue.record("Expected confirmed store state")
    return
  }
  #expect(state.collection.name == "Night")
  #expect(state.store.collections == [state.collection])
  let duplicate = await service.handle(
    try Wire.encode(Request(version: 1, id: 44, command: request.command)))
  guard case .collectionCreated(let other) = duplicate.event else {
    Issue.record("Expected a distinct created identity")
    return
  }
  #expect(other.collection.id != state.collection.id)
  #expect(other.store.collections.count == 2)
  let bad = Request(version: 1, id: 43, command: .collectionDelete(.init(id: "missing")))
  let error = await service.handle(try Wire.encode(bad))
  guard case .failure(let failure) = error.event else {
    Issue.record("Expected failure")
    return
  }
  #expect(failure.code == .notFound)
}

@MainActor @Test func savedReferencesCanBeEditedWithoutNetworkAccess() async throws {
  let service = Service()
  let library = try service.store.get()
  let song = sampleSong("saved")
  let state = try library.create(name: "Saved", description: "", items: [song])
  let id = state.collection.id
  let add = Request(version: 1, id: 1, command: .collectionAdd(.init(id: id, items: [song.ref])))
  let response = await service.handle(try Wire.encode(add))
  guard case .store(let updated) = response.event else {
    Issue.record("Expected store reply")
    return
  }
  #expect(updated.collections.first?.count == 2)
  let favorite = Request(
    version: 1, id: 2, command: .favorite(.init(item: song.ref, enabled: true)))
  let result = await service.handle(try Wire.encode(favorite))
  guard case .store(let favorites) = result.event else {
    Issue.record("Expected favorites")
    return
  }
  #expect(favorites.favorites == [song.ref])
  let copy = Request(
    version: 1, id: 3,
    command: .collectionCopy(
      .init(
        item: .init(id: id, source: .collection, kind: .playlist), name: "Copy")))
  let copied = await service.handle(try Wire.encode(copy))
  guard case .collectionCreated(let copies) = copied.event else {
    Issue.record("Expected copied collection")
    return
  }
  #expect(copies.store.collections.count == 2)
  #expect(copies.collection.id != id)
  #expect(copies.collection == copies.store.collections[1])
  #expect(library.state.collections[1].items == [song, song])
  let create = Request(
    version: 1, id: 4,
    command: .collectionCreate(
      .init(name: "With songs", description: "", items: [song.ref, song.ref])))
  let created = await service.handle(try Wire.encode(create))
  guard case .collectionCreated(let playlist) = created.event else {
    Issue.record("Expected creation with initial songs")
    return
  }
  #expect(playlist.collection.count == 2)
  #expect(library.state.collections.last?.items == [song, song])
}
