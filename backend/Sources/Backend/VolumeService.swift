import CoreAudio
import Foundation
import MusicContract

@MainActor
final class VolumeService {
  var onChange: ((VolumeState) -> Void)?
  private var listeners:
    [(AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertyListenerBlock)] = []
  private var active = false

  private func address(
    _ selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope = kAudioDevicePropertyScopeOutput,
    element: AudioObjectPropertyElement = kAudioObjectPropertyElementMain
  ) -> AudioObjectPropertyAddress {
    .init(mSelector: selector, mScope: scope, mElement: element)
  }

  private func read<T>(_ device: AudioObjectID, _ property: AudioObjectPropertyAddress, initial: T)
    -> T?
  {
    var property = property
    var value = initial
    var size = UInt32(MemoryLayout<T>.size)
    let status = withUnsafeMutablePointer(to: &value) {
      AudioObjectGetPropertyData(device, &property, 0, nil, &size, $0)
    }
    return status == noErr && size == MemoryLayout<T>.size ? value : nil
  }

  private func writable(_ device: AudioObjectID, _ property: AudioObjectPropertyAddress) -> Bool {
    var property = property
    var result = DarwinBoolean(false)
    return AudioObjectIsPropertySettable(device, &property, &result) == noErr && result.boolValue
  }

  private func write<T>(_ device: AudioObjectID, _ property: AudioObjectPropertyAddress, value: T)
    throws
  {
    var property = property
    var value = value
    let status = withUnsafePointer(to: &value) {
      AudioObjectSetPropertyData(device, &property, 0, nil, UInt32(MemoryLayout<T>.size), $0)
    }
    guard status == noErr else {
      throw Failure(code: .unavailable, message: "장치 음량을 변경할 수 없습니다", retryable: true)
    }
  }

  private func device() -> AudioObjectID {
    read(
      AudioObjectID(kAudioObjectSystemObject),
      address(kAudioHardwarePropertyDefaultOutputDevice, scope: kAudioObjectPropertyScopeGlobal),
      initial: AudioObjectID(kAudioObjectUnknown)) ?? AudioObjectID(kAudioObjectUnknown)
  }

  private func name(_ device: AudioObjectID) -> String {
    let value: Unmanaged<CFString>?? = read(
      device,
      address(kAudioObjectPropertyName, scope: kAudioObjectPropertyScopeGlobal),
      initial: Optional<Unmanaged<CFString>>.none)
    return (value ?? nil)?.takeRetainedValue() as String? ?? "출력 장치 없음"
  }

  private func levels(_ device: AudioObjectID) -> [(element: UInt32, value: Float32)] {
    let main = address(kAudioDevicePropertyVolumeScalar)
    let master = read(device, main, initial: Float32(0))
    if let master, writable(device, main) { return [(0, master)] }
    if let pair = read(
      device, address(kAudioDevicePropertyPreferredChannelsForStereo),
      initial: (UInt32(0), UInt32(0)))
    {
      let elements = pair.0 == pair.1 ? [pair.0] : [pair.0, pair.1]
      let values = elements.compactMap { element -> (UInt32, Float32)? in
        guard
          let value = read(
            device, address(kAudioDevicePropertyVolumeScalar, element: element), initial: Float32(0)
          )
        else { return nil }
        return (element, value)
      }
      if values.count == elements.count { return values }
    }
    return master.map { [(UInt32(0), $0)] } ?? []
  }

  func state() -> VolumeState {
    state(device())
  }

  private func state(_ device: AudioObjectID) -> VolumeState {
    let values = levels(device)
    let level = values.map(\.value).max().flatMap { $0.isFinite ? Double(min(1, max(0, $0))) : nil }
    let muteAddress = address(kAudioDevicePropertyMute)
    let mute = read(device, muteAddress, initial: UInt32(0))
    return .init(
      device: name(device), level: level, muted: mute.map { $0 != 0 },
      canSetVolume: level != nil
        && values.allSatisfy {
          writable(device, address(kAudioDevicePropertyVolumeScalar, element: $0.element))
        },
      canMute: mute != nil && writable(device, muteAddress))
  }

  static func scaled(_ values: [Float32], to level: Double) throws -> [Float32] {
    guard level.isFinite && (0...1).contains(level),
      values.allSatisfy({ $0.isFinite && (0...1).contains($0) })
    else {
      throw Failure(code: .invalidRequest, message: "음량 범위를 확인해주세요", retryable: false)
    }
    let peak = values.max() ?? 0
    // With all channels at zero there is no observable balance; start equally.
    return values.map { peak > 0 ? Float32(level) * $0 / peak : Float32(level) }
  }

  func set(_ params: VolumeParams) throws -> VolumeState {
    if let level = params.level { _ = try Self.scaled([], to: level) }
    let device = device()
    let current = state(device)
    guard (params.level == nil || current.canSetVolume) && (params.muted == nil || current.canMute)
    else {
      throw Failure(code: .unavailable, message: "이 장치에서는 조절할 수 없습니다", retryable: false)
    }
    let before = levels(device)
    guard params.level == nil || !before.isEmpty, device == self.device() else {
      throw Failure(code: .conflict, message: "출력 장치가 변경되었습니다", retryable: true)
    }
    do {
      if let level = params.level {
        let values = try Self.scaled(before.map(\.value), to: level)
        for (channel, value) in zip(before, values) {
          try write(
            device, address(kAudioDevicePropertyVolumeScalar, element: channel.element),
            value: value)
        }
      }
      if let muted = params.muted {
        try write(device, address(kAudioDevicePropertyMute), value: UInt32(muted ? 1 : 0))
      }
    } catch {
      for channel in before {
        try? write(
          device, address(kAudioDevicePropertyVolumeScalar, element: channel.element),
          value: channel.value)
      }
      onChange?(state())
      throw error
    }
    // HAL may apply asynchronously; the listener publishes the confirmed new value.
    let value = state()
    onChange?(value)
    return value
  }

  func start() {
    active = true
    observe()
  }

  func close() {
    active = false
    removeListeners()
    onChange = nil
  }

  private func removeListeners() {
    for (device, original, block) in listeners {
      var property = original
      AudioObjectRemovePropertyListenerBlock(device, &property, .main, block)
    }
    listeners.removeAll()
  }

  private func observe() {
    removeListeners()
    watch(
      AudioObjectID(kAudioObjectSystemObject),
      address(
        kAudioHardwarePropertyDefaultOutputDevice,
        scope: kAudioObjectPropertyScopeGlobal), rebind: true)
    let device = device()
    watch(
      device, address(kAudioObjectPropertyOwnedObjects, scope: kAudioObjectPropertyScopeGlobal),
      rebind: true)
    watch(device, address(kAudioDevicePropertyMute))
    for channel in levels(device) {
      watch(device, address(kAudioDevicePropertyVolumeScalar, element: channel.element))
    }
  }

  private func watch(
    _ device: AudioObjectID, _ original: AudioObjectPropertyAddress, rebind: Bool = false
  ) {
    var property = original
    guard AudioObjectHasProperty(device, &property) else { return }
    let block: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
      Task { @MainActor [weak self] in
        guard let self, self.active else { return }
        if rebind { self.observe() }
        self.onChange?(self.state())
      }
    }
    if AudioObjectAddPropertyListenerBlock(device, &property, .main, block) == noErr {
      listeners.append((device, property, block))
    }
  }
}
