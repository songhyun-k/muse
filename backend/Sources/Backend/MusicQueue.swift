import Foundation
import MusicContract
@preconcurrency import MusicKit

extension MusicService {
  func upcoming() -> [MusicPlayer.Queue.Entry] {
    guard let player else { return [] }
    let entries = Array(player.queue.entries)
    let index = entries.firstIndex { $0.id == player.queue.currentEntry?.id }
    return Array(entries.dropFirst(index.map { $0 + 1 } ?? 0))
  }

  func queuePage(_ params: QueueParams) throws -> QueuePage {
    let state = playbackState()
    if let revision = params.revision, revision != state.queueRevision {
      onPlayback?(state)
      throw queueConflict()
    }
    let entries = upcoming()
    let start = min(Int(params.offset), entries.count)
    let end = min(start + 50, entries.count)
    let page = entries[start..<end].map { QueueEntry(id: $0.id, item: item(for: $0)) }
    return .init(
      entries: page, nextOffset: end < entries.count ? UInt64(end) : nil,
      revision: queueRevision, total: UInt64(entries.count))
  }

  func removeQueueEntry(_ id: String) throws -> PlayerState {
    guard let player, upcoming().contains(where: { $0.id == id }) else { throw queueConflict() }
    player.queue.entries.removeAll { $0.id == id }
    entryMetadata.removeValue(forKey: id)
    return playbackChanged()
  }

  func moveQueueEntry(_ params: QueueMove) throws -> PlayerState {
    let future = upcoming()
    guard let player, future.contains(where: { $0.id == params.entryId }),
      params.beforeEntryId.map({ before in future.contains { $0.id == before } }) ?? true
    else { throw queueConflict() }
    let result = try Self.moving(
      Array(player.queue.entries), id: params.entryId, before: params.beforeEntryId)
    player.queue.entries = .init(result)
    return playbackChanged()
  }

  static func moving(_ entries: [MusicPlayer.Queue.Entry], id: String, before: String?) throws
    -> [MusicPlayer.Queue.Entry]
  {
    guard let index = entries.firstIndex(where: { $0.id == id }) else { throw queueConflict() }
    if before == id { return entries }
    var result = entries
    let entry = result.remove(at: index)
    let destination: Int
    if let before {
      guard let index = result.firstIndex(where: { $0.id == before }) else { throw queueConflict() }
      destination = index
    } else {
      destination = result.count
    }
    result.insert(entry, at: destination)
    return result
  }

  func jumpQueueEntry(_ id: String) async throws -> PlayerState {
    guard let player else { throw queueConflict() }
    if player.queue.currentEntry?.id == id { return playbackState() }
    guard let entry = upcoming().first(where: { $0.id == id }) else { throw queueConflict() }
    player.queue.currentEntry = entry
    radioPlaying = entryMetadata[id]?.ref.kind == .station
    try await player.play()
    return playbackChanged()
  }

  func clearQueue() -> PlayerState {
    player?.stop()
    player?.queue.entries = []
    entryMetadata.removeAll()
    radioPlaying = false
    return playbackChanged()
  }

}

private func queueConflict() -> Failure {
  .init(code: .conflict, message: "재생 큐가 변경되었습니다. 다시 선택해주세요", retryable: true)
}
