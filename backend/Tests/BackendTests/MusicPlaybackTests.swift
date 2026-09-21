import Foundation
import MusicContract
import MusicKit
import Testing

@testable import Backend

@MainActor @Test func idlePlaybackDoesNotInitializeNativePlayer() async throws {
  let music = MusicService()
  let state = music.playbackState()
  #expect(music.player == nil)
  #expect(!state.playing && state.current == nil && state.queueCount == 0)
  #expect(state.updatedAt > 0)
  #expect(try Wire.encode(Event(version: 1, sequence: 1, event: .player(state))).count < 1024)
  _ = try await music.control(.pause)
  #expect(music.player == nil)
  #expect(throws: Failure.self) { try music.seek(.init(seconds: 1, entryId: "missing")) }
  music.closePlayer()
}

@MainActor @Test func seekAndRepeatRespectNativeCapabilities() throws {
  var state = MusicService().playbackState()
  state.current = sampleSong("seek")
  state.current?.duration = 208
  state.currentEntryId = "entry"
  state.canSeek = true
  try MusicService.validateSeek(.init(seconds: 0, entryId: "entry"), state: state)
  try MusicService.validateSeek(.init(seconds: 208, entryId: "entry"), state: state)
  for seconds in [-1, 209, Double.nan, Double.infinity] {
    #expect(throws: Failure.self) {
      try MusicService.validateSeek(.init(seconds: seconds, entryId: "entry"), state: state)
    }
  }
  #expect(throws: Failure.self) {
    try MusicService.validateSeek(.init(seconds: 10, entryId: "previous-entry"), state: state)
  }
  state.canSeek = false
  #expect(throws: Failure.self) {
    try MusicService.validateSeek(.init(seconds: 10, entryId: "entry"), state: state)
  }
  #expect(MusicService.repeatMode(MusicPlayer.RepeatMode.none) == .off)
  #expect(MusicService.repeatMode(.one) == .one)
  #expect(MusicService.repeatMode(.all) == .all)
}

@MainActor @Test func duplicateSongsReceiveDistinctNativeQueueIdentities() throws {
  let data = Data(
    """
    {"id":"fixture-queue","type":"songs","attributes":{
      "name":"노래","artistName":"가수","albumName":"앨범","durationInMillis":208000,
      "genreNames":[],"trackNumber":1,"discNumber":1,"hasLyrics":false,
      "playParams":{"id":"fixture-queue","kind":"song"}}}
    """.utf8)
  let song = try JSONDecoder().decode(Song.self, from: data)
  let first = MusicPlayer.Queue.Entry(song)
  let second = MusicPlayer.Queue.Entry(song)
  #expect(first.id != second.id)
  let reordered = try MusicService.moving([first, second], id: first.id, before: nil)
  #expect(reordered.map(\.id) == [second.id, first.id])
  #expect(
    try MusicService.moving(reordered, id: first.id, before: second.id).map(\.id) == [
      first.id, second.id,
    ])
  #expect(throws: Failure.self) {
    try MusicService.moving([first, second], id: first.id, before: "missing")
  }
}

@MainActor @Test func queueRevisionRejectsStalePages() throws {
  let music = MusicService()
  let page = try music.queuePage(.init(offset: 0, revision: 0))
  #expect(page.entries.isEmpty && page.total == 0 && page.nextOffset == nil)
  var published: [PlayerState] = []
  music.onPlayback = { published.append($0) }
  music.queueRevision = 1
  #expect(throws: Failure.self) { try music.queuePage(.init(offset: 0, revision: 0)) }
  #expect(published.map(\.queueRevision) == [1])
  #expect(music.player == nil)
  #expect(try music.queuePage(.init(offset: 0, revision: 1)).revision == 1)
  #expect(published.count == 1)
  #expect(throws: Failure.self) { try music.removeQueueEntry("missing") }
}

@MainActor @Test func historyRecordsEntryChangesWithoutCountingProgressTicks() throws {
  let service = Service()
  let song = sampleSong("history")
  var state = service.music.playbackState()
  state.current = song
  state.currentEntryId = "first"
  state.playing = true
  service.playbackChanged(state)
  state.position = 20
  service.playbackChanged(state)
  state.playing = false
  service.playbackChanged(state)
  state.playing = true
  service.playbackChanged(state)
  #expect(try service.store.get().state.history == [song])
  state.currentEntryId = "second"
  service.playbackChanged(state)
  #expect(try service.store.get().state.history == [song, song])
  service.close()
}

@MainActor @Test func historyRetriesTheSameEntryAfterStorageRecovers() throws {
  let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  defer { try? FileManager.default.removeItem(at: directory) }
  let file = directory.appendingPathComponent("library.json")
  var owner: LibraryStore? = try LibraryStore(file: file)
  let song = sampleSong("retry-history")
  try owner?.favorite(song, enabled: true)
  let service = Service(storeURL: file, music: MusicService(authorization: { .denied }))
  defer { service.close() }
  var failures = 0
  service.onEvent = { event in
    if case .failure(let failure) = event.event, failure.code == .storage { failures += 1 }
  }
  var state = service.music.playbackState()
  state.current = song
  state.currentEntryId = "same-entry"
  state.playing = true
  service.playbackChanged(state)
  service.playbackChanged(state)
  #expect(failures == 1)
  #expect(owner?.state.history.isEmpty == true)
  owner = nil
  service.playbackChanged(state)
  service.playbackChanged(state)
  let saved = try JSONDecoder().decode(SavedLibrary.self, from: Data(contentsOf: file))
  #expect(saved.history == [song] && saved.favorites == [song])
  #expect(failures == 1)
  // A later write failure must re-arm reporting and retry the same queue entry too.
  try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: directory.path)
  defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path) }
  state.currentEntryId = "next-entry"
  service.playbackChanged(state)
  service.playbackChanged(state)
  #expect(failures == 2)
  try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
  service.playbackChanged(state)
  service.playbackChanged(state)
  #expect(try service.store.get().state.history == [song, song])
}
