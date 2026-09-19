import Backend
import CMuse
import Foundation
import HostTransport
import MusicContract

@MainActor
private protocol BackendProvider: AnyObject {
  var onEvent: (@MainActor (Event) -> Void)? { get set }
  func submit(_ data: Data, reply: @escaping @MainActor (Event) -> Void)
  func start()
  func close()
}
extension Service: BackendProvider {}
extension DemoService: BackendProvider {}

private func mailbox(_ context: UnsafeMutableRawPointer?) -> Mailbox? {
  context.map { Unmanaged<Mailbox>.fromOpaque($0).takeUnretainedValue() }
}

private func submit(
  _ context: UnsafeMutableRawPointer?, _ bytes: UnsafePointer<UInt8>?, _ count: Int
) -> Int32 {
  guard let box = mailbox(context), let bytes, count > 0, count <= maxMessageBytes else {
    return Admission.invalid.rawValue
  }
  return box.submit(Data(bytes: bytes, count: count)).rawValue
}

private func receive(
  _ context: UnsafeMutableRawPointer?, _ bytes: UnsafeMutablePointer<UInt8>?, _ capacity: Int
) -> Int32 {
  guard let box = mailbox(context), let bytes, capacity >= 0 else { return -1 }
  switch box.receive(capacity: capacity) {
  case .closed: return -1
  case .idle: return 0
  case .required(let size): return Int32(size)
  case .message(let data):
    data.copyBytes(to: bytes, count: data.count)
    return Int32(data.count)
  }
}

@main
struct MusicHost {
  @MainActor
  static func main() async {
    let arguments = Array(CommandLine.arguments.dropFirst())
    if arguments.contains("--live-check") { exit(await NativeAcceptance.run(arguments)) }
    let diagnostic = arguments.contains { ["--probe", "--version", "--help"].contains($0) }
    let box = Mailbox()
    let service: any BackendProvider
    do {
      if arguments.contains("--demo") {
        service = try DemoService()
      } else {
        var directory = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(
          "Library/Application Support/muse")
        if !diagnostic, let path = ProcessInfo.processInfo.environment["MUSE_STATE_DIR"] {
          guard path.hasPrefix("/") else {
            fputs("MUSE_STATE_DIR must be an absolute path\n", stderr)
            exit(2)
          }
          directory = URL(fileURLWithPath: path, isDirectory: true)
        }
        service = Service(
          storeURL: diagnostic ? nil : directory.appendingPathComponent("library.json"))
      }
    } catch {
      FileHandle.standardError.write(Data("Could not read demo data\n".utf8))
      exit(1)
    }
    service.onEvent = { event in
      if let data = try? Wire.encode(event) { box.publish(event.event.tag, data) }
    }
    if !diagnostic { service.start() }
    let server = Task {
      for await request in box.requests {
        guard !Task.isCancelled else { break }
        service.submit(request) { reply in
          var event = reply
          do {
            box.complete(try Wire.encode(event))
          } catch {
            event.event = .failure(
              .init(code: .internalError, message: "응답을 전달할 수 없습니다", retryable: true))
            if let data = try? Wire.encode(event) { box.complete(data) }
          }
        }
      }
    }
    guard let options = try? JSONEncoder().encode(arguments) else { exit(2) }
    let result: Int32 = await withCheckedContinuation { continuation in
      Thread.detachNewThread {
        let context = Unmanaged.passUnretained(box).toOpaque()
        let bridge = MuseBridge(abi_version: 1, context: context, submit: submit, receive: receive)
        let code = options.withUnsafeBytes { buffer in
          muse_run(bridge, buffer.bindMemory(to: UInt8.self).baseAddress, options.count)
        }
        box.close()
        continuation.resume(returning: code)
      }
    }
    server.cancel()
    service.close()
    exit(result)
  }
}
