import Combine
import Foundation
import MusicContract
@preconcurrency import MusicKit

extension MusicService {
  func playbackState() -> PlayerState {
    guard let player else {
      return .init(
        playing: false, position: 0, queueCount: 0, queueRevision: queueRevision,
        updatedAt: Date().timeIntervalSince1970, repeatMode: .off, shuffle: false, canSeek: false)
    }
    let entries = Array(player.queue.entries)
    let current = player.queue.currentEntry
    let fingerprint = [current?.id ?? ""] + entries.flatMap { [$0.id, $0.item?.id.rawValue ?? ""] }
    if fingerprint != queueFingerprint {
      queueFingerprint = fingerprint
      queueRevision += 1
      let present = Set(entries.map(\.id))
      entryMetadata = entryMetadata.filter { present.contains($0.key) }
    }
    let item = current.flatMap { self.item(for: $0) }
    let index = entries.firstIndex { $0.id == current?.id }
    let upcoming = entries.count - (index.map { $0 + 1 } ?? 0)
    let live = radioPlaying && current.flatMap { entryMetadata[$0.id]?.ref.kind } != .song
    return .init(
      current: item, playing: player.state.playbackStatus == .playing,
      position: player.playbackTime.isFinite ? max(0, player.playbackTime) : 0,
      queueCount: UInt64(upcoming), queueRevision: queueRevision,
      updatedAt: Date().timeIntervalSince1970,
      currentEntryId: current?.id, repeatMode: Self.repeatMode(player.state.repeatMode),
      shuffle: player.state.shuffleMode == .songs, canSeek: !live && (item?.duration ?? 0) > 0)
  }

  func item(for entry: MusicPlayer.Queue.Entry) -> Item? {
    if case .song(let song) = entry.item {
      let source = entryMetadata[entry.id]?.ref.source ?? .catalog
      var item = remember(.song(song), source: source)
      if let original = entryMetadata[entry.id], original.ref.kind == .song {
        item.ref = original.ref
      }
      return item
    }
    return entryMetadata[entry.id]
  }

  func play(_ references: [ItemRef], startIndex: Int, placement: Placement, shuffle: Bool? = nil)
    async throws
    -> PlayerState
  {
    try requireAuthorization()
    guard !references.isEmpty, references.count <= 2000, references.indices.contains(startIndex)
    else {
      throw Failure(code: .invalidRequest, message: "재생할 곡을 선택해주세요", retryable: false)
    }
    let station = references.contains { $0.kind == .station }
    guard !station || (references.count == 1 && placement == .replace) else {
      throw Failure(code: .unavailable, message: "스테이션은 바로 재생해주세요", retryable: false)
    }
    if references.contains(where: { $0.source == .catalog }) {
      catalogPlayable = try await MusicSubscription.current.canPlayCatalogContent
      guard catalogPlayable == true else {
        throw Failure(
          code: .subscriptionRequired, message: "Apple Music 구독을 확인해주세요", retryable: false)
      }
    }
    var entries: [MusicPlayer.Queue.Entry] = []
    var metadata: [String: Item] = [:]
    for (reference, entity) in zip(references, try await resolve(references)) {
      let entry: MusicPlayer.Queue.Entry
      switch entity {
      case .song(let song): entry = .init(song)
      case .station(let station): entry = .init(station)
      default: throw Failure(code: .unavailable, message: "재생할 노래를 선택해주세요", retryable: false)
      }
      entries.append(entry)
      metadata[entry.id] = entity.item(source: reference.source)
    }
    try Task.checkCancellation()
    let player = self.player ?? ApplicationMusicPlayer.shared
    self.player = player
    let previousQueue = player.queue
    let previousMetadata = entryMetadata
    let previousTime = player.playbackTime
    let wasPlaying = player.state.playbackStatus == .playing
    let previousRadio = radioPlaying
    let previousShuffle = player.state.shuffleMode
    do {
      if placement == .replace || player.queue.entries.isEmpty {
        // Select after assignment: startingAt can reject a newly prepared start item.
        player.queue = .init(entries)
        if startIndex > 0 { player.queue.currentEntry = entries[startIndex] }
        entryMetadata = metadata
        radioPlaying = station
      } else {
        try await player.queue.insert(
          entries, position: placement == .next ? .afterCurrentEntry : .tail)
        entryMetadata.merge(metadata) { _, new in new }
      }
      if let shuffle { player.state.shuffleMode = shuffle ? .songs : .off }
      observePlayer()
      if placement == .replace { try await player.play() }
      return playbackChanged()
    } catch {
      if placement == .replace {
        player.queue = previousQueue
        entryMetadata = previousMetadata
        radioPlaying = previousRadio
        player.state.shuffleMode = previousShuffle
        player.playbackTime = previousTime
        if wasPlaying { try? await player.play() } else { player.pause() }
      }
      observePlayer()
      _ = playbackChanged()
      throw error
    }
  }

  func control(_ action: Control) async throws -> PlayerState {
    guard let player else {
      if action == .stop || action == .pause { return playbackState() }
      throw Failure(code: .notFound, message: "재생할 곡을 선택해주세요", retryable: false)
    }
    switch action {
    case .toggle:
      if player.state.playbackStatus == .playing { player.pause() } else { try await player.play() }
    case .pause: player.pause()
    case .stop: player.stop()
    case .next: try await player.skipToNextEntry()
    case .previous:
      if player.playbackTime > 3 {
        player.restartCurrentEntry()
      } else {
        try await player.skipToPreviousEntry()
      }
    }
    return playbackChanged()
  }

  static func validateSeek(_ params: SeekParams, state: PlayerState) throws {
    guard params.entryId == state.currentEntryId else {
      throw Failure(code: .conflict, message: "재생 중인 곡이 변경되었습니다", retryable: true)
    }
    guard state.canSeek, let duration = state.current?.duration, params.seconds.isFinite,
      params.seconds >= 0, params.seconds <= duration
    else {
      throw Failure(code: .unavailable, message: "이 위치로 이동할 수 없습니다", retryable: false)
    }
  }

  func seek(_ params: SeekParams) throws -> PlayerState {
    let state = playbackState()
    try Self.validateSeek(params, state: state)
    player?.playbackTime = params.seconds
    return playbackChanged()
  }

  func mode(_ params: ModeParams) throws -> PlayerState {
    guard let player else {
      throw Failure(code: .notFound, message: "재생할 곡을 선택해주세요", retryable: false)
    }
    player.state.shuffleMode = params.shuffle ? .songs : .off
    switch params.repeatMode {
    case .off: player.state.repeatMode = MusicPlayer.RepeatMode.none
    case .one: player.state.repeatMode = .one
    case .all: player.state.repeatMode = .all
    }
    return playbackChanged()
  }

  static func repeatMode(_ mode: MusicPlayer.RepeatMode?) -> RepeatMode {
    switch mode {
    case .one: .one
    case .all: .all
    default: .off
    }
  }

  func observePlayer() {
    guard let player else { return }
    playbackObservers = [player.state.objectWillChange, player.queue.objectWillChange].map {
      publisher in
      publisher.sink { [weak self] _ in
        Task { @MainActor [weak self] in
          await Task.yield()
          _ = self?.playbackChanged()
        }
      }
    }
  }

  @discardableResult
  func playbackChanged() -> PlayerState {
    let state = playbackState()
    onPlayback?(state)
    if state.playing && positionTask == nil {
      positionTask = Task { [weak self] in
        while !Task.isCancelled {
          do { try await Task.sleep(for: .milliseconds(500)) } catch { break }
          guard let self else { return }
          guard self.player?.state.playbackStatus == .playing else { break }
          self.onPlayback?(self.playbackState())
        }
        if !Task.isCancelled { self?.positionTask = nil }
      }
    } else if !state.playing {
      positionTask?.cancel()
      positionTask = nil
    }
    return state
  }

  func closePlayer() {
    onPlayback = nil
    onFailure = nil
    positionTask?.cancel()
    positionTask = nil
    playbackObservers.removeAll()
    player?.stop()
    player = nil
  }
}
