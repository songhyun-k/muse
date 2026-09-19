import Foundation
import MusicContract
import Testing

@testable import Backend

@MainActor @Test func volumeScalingPreservesChannelBalance() throws {
  let result = try VolumeService.scaled([0.2, 0.4], to: 0.8)
  #expect(abs(result[0] - 0.4) < 0.0001 && abs(result[1] - 0.8) < 0.0001)
  #expect(try VolumeService.scaled([0, 0], to: 0.5) == [0.5, 0.5])
  #expect(throws: Failure.self) { try VolumeService.scaled([0.2], to: .nan) }
  #expect(throws: Failure.self) { try VolumeService.scaled([0.2], to: 1.1) }
}

@MainActor @Test func actualDeviceReadIsCapabilityAware() {
  // Deliberately read-only: this check never changes the user's audio settings.
  let service = VolumeService()
  let state = service.state()
  if let level = state.level { #expect(level.isFinite && (0...1).contains(level)) }
  #expect(!state.canSetVolume || state.level != nil)
  #expect(!state.canMute || state.muted != nil)
  service.close()
}
