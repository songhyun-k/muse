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
