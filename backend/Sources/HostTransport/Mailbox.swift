import Foundation
import MusicContract

public enum Admission: Int32, Sendable {
  case accepted = 0
  case busy = 1
  case closed = 2
  case invalid = 3
}

public enum Delivery: Equatable, Sendable {
  case idle, closed
  case required(Int)
  case message(Data)
}

/// All mutable fields are protected by lock; no lock crosses an await/callback.
public final class Mailbox: @unchecked Sendable {
  public let requests: AsyncStream<Data>
  public let exits: AsyncStream<Int32>
  private let exitInput: AsyncStream<Int32>.Continuation
  private var exiting = false
  private let input: AsyncStream<Data>.Continuation
  private let lock = NSLock()
  private var closed = false
  private var outstanding = 0
  private var replies: [Data] = []
  private var events: [String: Data] = [:]
  private var eventOrder: [String] = []
  private var preferEvent = false

  public init() {
    (requests, input) = AsyncStream.makeStream(bufferingPolicy: .bufferingOldest(16))
    (exits, exitInput) = AsyncStream.makeStream(bufferingPolicy: .bufferingOldest(1))
  }

  public func requestExit(_ code: Int32) {
    lock.withLock {
      guard !exiting else { return }
      exiting = true
      exitInput.yield(code)
      exitInput.finish()
    }
  }

  public func submit(_ data: Data) -> Admission {
    guard !data.isEmpty && data.count <= maxMessageBytes else { return .invalid }
    return lock.withLock {
      guard !closed else { return .closed }
      guard outstanding < 16 else { return .busy }
      switch input.yield(data) {
      case .enqueued:
        outstanding += 1
        return .accepted
      case .dropped: return .busy
      case .terminated: return .closed
      @unknown default: return .closed
      }
    }
  }

  @discardableResult
  public func complete(_ data: Data) -> Bool {
    lock.withLock {
      guard !closed, data.count <= maxMessageBytes, replies.count < outstanding else {
        return false
      }
      replies.append(data)
      return true
    }
  }

  @discardableResult
  public func publish(_ key: String, _ data: Data) -> Bool {
    lock.withLock {
      guard !closed, data.count <= maxMessageBytes else { return false }
      if events[key] == nil {
        guard events.count < 16 else { return false }
        eventOrder.append(key)
      }
      events[key] = data
      return true
    }
  }

  public func receive(capacity: Int) -> Delivery {
    lock.withLock {
      guard !closed else { return .closed }
      let useEvent = !eventOrder.isEmpty && (preferEvent || replies.isEmpty)
      let data = useEvent ? events[eventOrder[0]] : replies.first
      guard let data else { return .idle }
      guard data.count <= capacity else { return .required(data.count) }
      if useEvent {
        events.removeValue(forKey: eventOrder.removeFirst())
      } else {
        replies.removeFirst()
        outstanding -= 1
      }
      preferEvent = !useEvent
      return .message(data)
    }
  }

  public func close() {
    lock.withLock {
      closed = true
      replies.removeAll()
      events.removeAll()
      eventOrder.removeAll()
      input.finish()
    }
  }
}
