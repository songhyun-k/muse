import Foundation
import MusicContract
import MusicKit
import Testing

@testable import Backend

@MainActor @Test func authorizationStatesRemainDistinct() {
  #expect(MusicService.authorization(.authorized) == .authorized)
  #expect(MusicService.authorization(.denied) == .denied)
  #expect(MusicService.authorization(.restricted) == .restricted)
  #expect(MusicService.authorization(.notDetermined) == .notDetermined)
}

@MainActor @Test func nativeSongMetadataPreservesOpaqueIdentity() async throws {
  let data = Data(
    """
    {"id":"fixture-song","type":"songs","attributes":{
      "name":"노래","artistName":"가수","albumName":"앨범","durationInMillis":208000,
      "genreNames":["Pop"],"releaseDate":"2021-04-27","trackNumber":1,"discNumber":1,
      "hasLyrics":true,"isAppleDigitalMaster":false}}
    """.utf8)
  let song = try JSONDecoder().decode(Song.self, from: data)
  let music = MusicService()
  let catalog = music.remember(.song(song), source: .catalog)
  let library = music.remember(.song(song), source: .library)
  #expect(catalog.title == "노래")
  #expect(catalog.duration == 208)
  #expect(catalog.ref.id == "fixture-song")
  #expect(catalog.ref != library.ref)
  let page = music.page(
    MusicItemCollection([song]), source: .catalog, offset: 0, entity: MusicEntity.song)
  #expect(page.items == [catalog])
  #expect(page.nextOffset == nil)
  let resolved = try await music.resolve(library.ref)
  #expect(resolved.item(source: .library).ref == library.ref)
  let duplicates = MusicItemCollection(Array(repeating: song, count: 53))
  let first = try await music.relationshipPage(
    duplicates, source: .catalog, offset: 0, entity: MusicEntity.song)
  let last = try await music.relationshipPage(
    duplicates, source: .catalog, offset: 50, entity: MusicEntity.song)
  #expect(first.items.count == 50 && first.nextOffset == 50)
  #expect(last.items.count == 3 && last.nextOffset == nil)
  let all = try await music.relationshipPage(
    duplicates, source: .catalog, offset: 0, limit: 2001, entity: MusicEntity.song)
  #expect(all.items.count == 53 && all.nextOffset == nil)
}

@MainActor @Test func nativeErrorsProvideActionableFailuresWithoutLeakingProviderPayloads() async throws {
  let cases: [(any Error, ErrorCode, Bool, String)] = [
    (MusicTokenRequestError.developerTokenRequestFailed, .unavailable, false, "등록"),
    (MusicTokenRequestError.userNotSignedIn, .signInRequired, true, "로그인"),
    (MusicTokenRequestError.permissionDenied, .notAuthorized, false, "접근"),
    (MusicTokenRequestError.userTokenRevoked, .notAuthorized, false, "접근"),
    (MusicTokenRequestError.privacyAcknowledgementRequired, .notAuthorized, false, "개인정보"),
    (MusicTokenRequestError.userTokenRequestFailed, .unavailable, true, "계정"),
    (MusicTokenRequestError.unknown, .unavailable, true, "계정"),
    (MusicSubscription.Error.permissionDenied, .notAuthorized, false, "접근"),
    (MusicSubscription.Error.privacyAcknowledgementRequired, .notAuthorized, false, "개인정보"),
    (MusicSubscription.Error.unknown, .unavailable, true, "계정"),
    (MusicLibrary.Error.permissionDenied, .notAuthorized, false, "접근"),
    (URLError(.notConnectedToInternet), .network, true, "연결"),
    (URLError(.cancelled), .cancelled, true, "취소"),
  ]
  for (error, code, retryable, hint) in cases {
    let scheduler = RequestScheduler { _ in throw error }
    let data = try Wire.encode(Request(version: 1, id: 1, command: .snapshot(.init())))
    let event = await withCheckedContinuation { continuation in
      scheduler.submit(data) { continuation.resume(returning: $0) }
    }
    guard case .failure(let failure) = event.event else { Issue.record("Expected failure"); continue }
    #expect(failure.code == code && failure.retryable == retryable)
    #expect(failure.message.contains(hint))
    scheduler.close()
  }
  for (status, code, retryable) in [
    (400, ErrorCode.unavailable, false), (401, .unavailable, false),
    (403, .notAuthorized, false), (404, .notFound, false), (429, .busy, true),
    (408, .network, true), (500, .network, true), (503, .network, true),
  ] {
    let failure = RequestScheduler.musicHTTPFailure(status: status)
    #expect(failure.code == code && failure.retryable == retryable)
  }
}

@MainActor @Test func failedSubscriptionProbePreservesSuccessfulAuthorization() async {
  let music = MusicService()
  var failures: [Failure] = []
  music.onFailure = { failures.append($0) }
  let failed = await music.finishAuthorization(.authorized) {
    throw MusicSubscription.Error.privacyAcknowledgementRequired
  }
  #expect(failed.authorization == .authorized && failed.canPlayCatalog == nil)
  #expect(failures.map(\.code) == [.notAuthorized])
  let unsubscribed = await music.finishAuthorization(.authorized) { false }
  #expect(unsubscribed.authorization == .authorized && unsubscribed.canPlayCatalog == false)
  let allowed = await music.finishAuthorization(.authorized) { true }
  #expect(allowed.canPlayCatalog == true)
  let denied = await music.finishAuthorization(.denied) {
    Issue.record("Subscription must not be requested without permission")
    return true
  }
  #expect(denied.authorization == .denied && denied.canPlayCatalog == nil)
  #expect(failures.count == 1)
  music.closePlayer()
}
