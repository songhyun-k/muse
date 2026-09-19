import Foundation
import MusicContract
import MusicKit

@MainActor
final class RequestScheduler {
  typealias Reply = @MainActor (Event) -> Void
  private struct Job {
    let token: UUID
    let task: Task<Void, Never>
    let readOnly: Bool
    let reply: Reply
  }
  private var jobs: [UInt64: Job] = [:]
  private var running: Set<UUID> = []
  private var mutationTails: [MutationDomain: Task<Void, Never>] = [:]
  private var sequence: UInt64 = 0
  private var closed = false
  private let operation: @MainActor (Request) async throws -> Notice

  init(operation: @escaping @MainActor (Request) async throws -> Notice) {
    self.operation = operation
  }

  func submit(_ data: Data, reply: @escaping Reply) {
    var id: UInt64?
    do {
      let request = try Wire.decodeRequest(data)
      id = request.id
      guard !closed else { throw cancelled() }
      guard jobs[request.id] == nil else {
        throw Failure(code: .conflict, message: "이미 처리 중인 요청입니다", retryable: false)
      }
      if case .cancel(let params) = request.command {
        try cancel(params.requestId)
        emit(.ack(.init(message: "요청을 취소했습니다")), id: request.id, reply: reply)
        return
      }
      guard running.count < 16 else {
        throw Failure(code: .busy, message: "잠시 후 다시 시도해주세요", retryable: true)
      }
      let domains = request.command.mutationDomains
      let previous = domains.compactMap { mutationTails[$0] }
      let token = UUID()
      let operation = self.operation
      let task = Task { [weak self] in
        for task in previous { await task.value }
        let result: Result<Notice, Error>
        do {
          try Task.checkCancellation()
          result = .success(try await operation(request))
        } catch {
          result = .failure(error)
        }
        self?.finish(request.id, token: token, result: result)
      }
      jobs[request.id] = Job(token: token, task: task, readOnly: domains.isEmpty, reply: reply)
      running.insert(token)
      for domain in domains { mutationTails[domain] = task }
    } catch {
      let recovered = id ?? Wire.requestID(in: data)
      let available = recovered.flatMap { jobs[$0] == nil ? $0 : nil }
      emit(.failure(Self.failure(error)), id: available, reply: reply)
    }
  }

  func close() {
    closed = true
    for (id, job) in jobs.sorted(by: { $0.key < $1.key }) {
      job.task.cancel()
      emit(.failure(cancelled()), id: id, reply: job.reply)
    }
    jobs.removeAll()
    mutationTails.removeAll()
  }

  func notify(_ notice: Notice, reply: Reply) {
    guard !closed else { return }
    emit(notice, id: nil, reply: reply)
  }

  private func cancel(_ id: UInt64) throws {
    guard let job = jobs[id] else { return }
    guard job.readOnly else {
      throw Failure(code: .unavailable, message: "변경 작업은 완료될 때까지 기다려주세요", retryable: false)
    }
    jobs.removeValue(forKey: id)
    job.task.cancel()
    emit(.failure(cancelled()), id: id, reply: job.reply)
  }

  private func finish(_ id: UInt64, token: UUID, result: Result<Notice, Error>) {
    running.remove(token)
    guard let job = jobs[id], job.token == token else { return }
    jobs.removeValue(forKey: id)
    switch result {
    case .success(let notice): emit(notice, id: id, reply: job.reply)
    case .failure(let error): emit(.failure(Self.failure(error)), id: id, reply: job.reply)
    }
  }

  private func emit(_ notice: Notice, id: UInt64?, reply: Reply) {
    sequence += 1
    reply(.init(version: apiVersion, id: id, sequence: sequence, event: notice))
  }

  private func cancelled() -> Failure {
    .init(code: .cancelled, message: "요청이 취소되었습니다", retryable: true)
  }

  static func failure(_ error: Error) -> Failure {
    switch error {
    case let failure as Failure: return failure
    case StoreError.locked:
      return .init(code: .conflict, message: "다른 창에서 사용 중입니다", retryable: true)
    case is StoreError:
      return .init(code: .storage, message: "보관한 데이터를 읽거나 저장할 수 없습니다", retryable: true)
    case ContractError.unsupportedVersion:
      return .init(code: .unsupportedVersion, message: "지원하지 않는 통신 버전입니다", retryable: false)
    case is ContractError, is DecodingError:
      return .init(code: .invalidRequest, message: "요청 형식을 확인해주세요", retryable: false)
    case is CancellationError:
      return .init(code: .cancelled, message: "요청이 취소되었습니다", retryable: true)
    case let error as URLError:
      return error.code == .cancelled
        ? .init(code: .cancelled, message: "요청이 취소되었습니다", retryable: true)
        : .init(code: .network, message: "연결을 확인한 뒤 다시 시도해주세요", retryable: true)
    case let error as WebHTTPFailure: return musicHTTPFailure(status: error.status)
    case let error as MusicDataRequest.Error: return musicHTTPFailure(status: error.status)
    case MusicTokenRequestError.developerTokenRequestFailed:
      return .init(code: .unavailable, message: "이 앱의 Apple Music 등록을 확인해주세요", retryable: false)
    case MusicTokenRequestError.userNotSignedIn:
      return .init(code: .signInRequired, message: "음악 앱에 로그인해주세요", retryable: true)
    case MusicTokenRequestError.permissionDenied, MusicTokenRequestError.userTokenRevoked,
      MusicSubscription.Error.permissionDenied, MusicLibrary.Error.permissionDenied:
      return .init(code: .notAuthorized, message: "시스템 설정에서 음악 접근을 허용해주세요", retryable: false)
    case MusicTokenRequestError.privacyAcknowledgementRequired,
      MusicSubscription.Error.privacyAcknowledgementRequired:
      return .init(code: .notAuthorized, message: "음악 앱에서 개인정보 안내를 확인해주세요", retryable: false)
    case is MusicTokenRequestError, is MusicSubscription.Error, is MusicLibrary.Error:
      return .init(code: .unavailable, message: "Apple Music 계정을 확인한 뒤 다시 시도해주세요", retryable: true)
    default: return .init(code: .internalError, message: "요청을 처리할 수 없습니다", retryable: true)
    }
  }

  // MusicDataRequest.Error has no public initializer/Decodable conformance in the SDK.
  // Keep HTTP classification pure so all supported status paths can be checked offline.
  static func musicHTTPFailure(status: Int) -> Failure {
    switch status {
    case 401:
      return .init(code: .unavailable, message: "이 앱의 Apple Music 인증을 확인해주세요", retryable: false)
    case 403:
      return .init(code: .notAuthorized, message: "Apple Music 계정과 접근 권한을 확인해주세요", retryable: false)
    case 404:
      return .init(code: .notFound, message: "이 항목을 현재 지역에서 찾을 수 없습니다", retryable: false)
    case 429:
      return .init(code: .busy, message: "요청이 많습니다. 잠시 후 다시 시도해주세요", retryable: true)
    case 408, 500...599:
      return .init(code: .network, message: "Apple Music에 연결할 수 없습니다. 다시 시도해주세요", retryable: true)
    default:
      return .init(code: .unavailable, message: "Apple Music에서 이 요청을 지원하지 않습니다", retryable: false)
    }
  }
}

private enum MutationDomain {
  case library, playback, volume
}

extension Command {
  fileprivate var mutationDomains: [MutationDomain] {
    switch self {
    case .snapshot, .search, .browse, .detail, .queue, .lyrics, .lyricsSearch, .artwork, .cancel:
      []
    case .collectionCreate, .collectionUpdate, .collectionDelete, .collectionAdd,
      .collectionRemove, .collectionMove, .collectionCopy, .favorite, .lyricsChoose, .lyricsOffset:
      [.library]
    case .control, .seek, .mode, .queueRemove, .queueMove, .queueJump, .queueClear:
      [.playback]
    case .volume:
      [.volume]
    case .play, .authorize:
      // Play consumes saved collections; authorization affects provider access.
      // Keep their order with both domains without blocking independent volume changes.
      [.library, .playback]
    }
  }
}
