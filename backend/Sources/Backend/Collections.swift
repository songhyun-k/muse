import Foundation
import MusicContract

extension LibraryStore {
  var summary: StoreState {
    .init(
      collections: state.collections.map(\.summary), favorites: state.favorites.map(\.ref),
      historyCount: UInt64(state.history.count))
  }

  func recordPlayback(_ item: Item) throws {
    guard item.ref.kind == .song else { return }
    try transaction { $0.history = Array(([item] + $0.history).prefix(1000)) }
  }

  func lyricPreference(_ preference: LyricPreference, for reference: ItemRef) throws {
    try transaction { $0.lyrics[reference.storageKey] = preference }
  }

  @discardableResult
  func create(name: String, description: String, items: [Item] = []) throws -> CreatedCollection {
    guard state.collections.count < 100, items.count <= 2000 else { throw collectionLimit() }
    let collection = SavedCollection(
      id: UUID().uuidString, name: name.trimmingCharacters(in: .whitespacesAndNewlines),
      description: description, items: items)
    try transaction { $0.collections.append(collection) }
    return .init(collection: collection.summary, store: summary)
  }

  @discardableResult
  func update(_ params: CollectionUpdate) throws -> StoreState {
    let index = try collectionIndex(params.id)
    try transaction {
      if let name = params.name {
        $0.collections[index].name = name.trimmingCharacters(in: .whitespacesAndNewlines)
      }
      if let description = params.description { $0.collections[index].description = description }
    }
    return summary
  }

  @discardableResult
  func delete(_ id: String) throws -> StoreState {
    let index = try collectionIndex(id)
    try transaction { $0.collections.remove(at: index) }
    return summary
  }

  @discardableResult
  func add(_ items: [Item], to id: String) throws -> StoreState {
    guard items.allSatisfy({ $0.ref.kind == .song }) else {
      throw Failure(code: .unavailable, message: "노래를 선택해주세요", retryable: false)
    }
    let index = try collectionIndex(id)
    guard state.collections[index].items.count + items.count <= 2000 else {
      throw collectionLimit()
    }
    try transaction { $0.collections[index].items += items }
    return summary
  }

  @discardableResult
  func remove(_ params: CollectionRemove) throws -> StoreState {
    let index = try collectionIndex(params.id)
    guard params.index < state.collections[index].items.count else { throw staleSelection() }
    try transaction { $0.collections[index].items.remove(at: Int(params.index)) }
    return summary
  }

  @discardableResult
  func move(_ params: CollectionMove) throws -> StoreState {
    let index = try collectionIndex(params.id)
    let count = state.collections[index].items.count
    guard params.from < count && params.to < count else { throw staleSelection() }
    try transaction {
      let item = $0.collections[index].items.remove(at: Int(params.from))
      $0.collections[index].items.insert(item, at: Int(params.to))
    }
    return summary
  }

  @discardableResult
  func favorite(_ item: Item, enabled: Bool) throws -> StoreState {
    guard item.ref.kind == .song else {
      throw Failure(code: .unavailable, message: "노래를 선택해주세요", retryable: false)
    }
    let existing = state.favorites.firstIndex { $0.ref == item.ref }
    if (existing != nil) == enabled { return summary }
    guard !enabled || state.favorites.count < 2000 else { throw collectionLimit() }
    try transaction {
      if let existing { $0.favorites.remove(at: existing) }
      if enabled { $0.favorites.append(item) }
    }
    return summary
  }

  func localPage(_ params: BrowseParams) throws -> Page? {
    guard params.offset <= 1_000_000 else { throw staleSelection() }
    var items: [Item]
    switch params.scope {
    case .collections:
      items = state.collections.map {
        .init(
          ref: .init(id: $0.id, source: .collection, kind: .playlist), title: $0.name,
          artist: "", album: "", artworkUrl: $0.items.first?.artworkUrl)
      }
    case .collection:
      guard let id = params.collectionId else { throw staleSelection() }
      items = state.collections[try collectionIndex(id)].items
    case .favorites: items = state.favorites
    case .history: items = state.history
    default: return nil
    }
    if params.scope != .collection && params.order != .recent {
      items.sort {
        let first = params.order == .artist ? $0.artist : $0.title
        let second = params.order == .artist ? $1.artist : $1.title
        let comparison = first.localizedStandardCompare(second)
        return comparison == .orderedSame
          ? $0.ref.storageKey < $1.ref.storageKey : comparison == .orderedAscending
      }
    }
    let start = min(Int(params.offset), items.count)
    let end = min(start + 50, items.count)
    return .init(
      items: Array(items[start..<end]), nextOffset: end < items.count ? UInt64(end) : nil)
  }

  func savedItem(_ reference: ItemRef) -> Item? {
    for items in [state.favorites, state.history] + state.collections.map(\.items) {
      if let item = items.first(where: { $0.ref == reference }) { return item }
    }
    return nil
  }

  private func collectionIndex(_ id: String) throws -> Int {
    guard let index = state.collections.firstIndex(where: { $0.id == id }) else {
      throw Failure(code: .notFound, message: "플레이리스트를 찾을 수 없습니다", retryable: false)
    }
    return index
  }

  private func staleSelection() -> Failure {
    .init(code: .conflict, message: "목록이 변경되었습니다. 다시 선택해주세요", retryable: true)
  }

  private func collectionLimit() -> Failure {
    .init(code: .conflict, message: "저장할 수 있는 항목 수를 초과했습니다", retryable: false)
  }
}
