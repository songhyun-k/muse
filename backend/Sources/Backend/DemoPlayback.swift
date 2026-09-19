import Foundation
import MusicContract

extension DemoService {
  func revised() {
    player.queueCount = UInt64(queue.count)
    player.queueRevision += 1
  }

  func queueIndex(_ id: String) throws -> Int {
    guard let index = queue.firstIndex(where: { $0.id == id }) else { throw conflict() }
    return index
  }

  func enter(_ entry: QueueEntry, remember: Bool = true) throws {
    if remember, let item = player.current { past = Array((past + [item]).suffix(1000)) }
    player.current = entry.item
    player.currentEntryId = entry.id
    player.playing = true
    player.position = 0
    player.canSeek = entry.item?.duration != nil
    revised()
    if let item = entry.item {
      try store.recordPlayback(item)
      publish(.store(store.summary))
    }
  }

  func advance(to now: Double) {
    let elapsed = max(0, now - player.updatedAt)
    player.updatedAt = now
    guard player.playing else { return }
    player.position += elapsed
    if let duration = player.current?.duration, player.position >= duration {
      if player.repeatMode == .one {
        player.position = player.position.truncatingRemainder(dividingBy: max(1, duration))
      } else {
        do { try control(.next) } catch {
          player.playing = false
          publish(
            .failure(.init(code: .internalError, message: "데모 재생을 계속할 수 없습니다", retryable: true)))
        }
      }
    }
  }

  func control(_ action: Control) throws {
    switch action {
    case .toggle: player.playing.toggle()
    case .pause: player.playing = false
    case .stop:
      player.playing = false
      player.position = 0
    case .previous:
      if player.position > 3 || past.isEmpty {
        player.position = 0
        return
      }
      if let item = player.current { queue.insert(.init(id: UUID().uuidString, item: item), at: 0) }
      try enter(.init(id: UUID().uuidString, item: past.removeLast()), remember: false)
    case .next:
      if queue.isEmpty, player.repeatMode == .all {
        queue = (past + [player.current].compactMap { $0 }).map {
          .init(id: UUID().uuidString, item: $0)
        }
        past.removeAll()
      }
      guard !queue.isEmpty else {
        player.playing = false
        player.position = player.current?.duration ?? 0
        return
      }
      try enter(queue.removeFirst())
    }
  }

  func play(_ params: PlayParams) throws {
    var items: [Item] = []
    var start = 0
    for (index, reference) in params.items.enumerated() {
      if index == Int(params.startIndex) { start = items.count }
      items += try songs(reference)
      guard items.count <= 2000 else { throw conflict() }
    }
    guard start < items.count else { throw DemoCatalog.missing() }
    let entries = items.map { QueueEntry(id: UUID().uuidString, item: $0) }
    switch params.placement {
    case .replace:
      past = Array(items.prefix(start))
      queue = Array(entries.dropFirst(start + 1))
      player.shuffle = params.shuffle ?? player.shuffle
      if player.shuffle { queue.shuffle() }
      try enter(entries[start], remember: false)
    case .next, .last:
      guard queue.count + entries.count <= 2000 else { throw conflict() }
      queue.insert(contentsOf: entries, at: params.placement == .next ? 0 : queue.count)
      revised()
    }
  }
}
