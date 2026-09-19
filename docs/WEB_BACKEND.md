# 웹 조회 계층

검색·카탈로그 상세·홈은 Apple Music 웹 서비스에서 읽고, 재생과 보관함은
macOS MusicKit을 사용한다. 모든 요청은 Swift 백엔드가 담당한다.
화면에는 기존 계약의 Item/Page/Detail/Failure만 전달한다.

## 인증과 요청

1. AppleWeb이 music.apple.com/us/new와 제한된 메인 JavaScript에서 공개 웹 토큰을 읽는다.
2. MusicUserTokenProvider가 macOS의 기존 계정 인증으로 사용자 토큰을 제공한다.
3. WebMusic이 /v1/me/storefront에서 계정의 지역을 확인한다.
4. 고정된 amp-api.music.apple.com 경로에 Bearer·Media-User-Token과 웹 Origin/Referer를 보낸다.

사용자가 개발자 키를 발급하거나 토큰을 복사할 필요는 없다. macOS 음악 접근 권한과
계정 로그인이 필요하며, 카탈로그 재생은 Apple Music 구독 상태를 확인한다.
토큰은 앱 메모리에만 보관한다. 화면 계약, 저장 파일과 로그에 전달하지 않는다.
MusicKit의 전역 토큰 공급자는 변경하지 않는다.

## 경계와 실패 처리

- HTTPS 고정 호스트만 허용한다. 인증 헤더는 AMP API에만 보내며 리다이렉트는 거부한다.
- HTML 4 MiB, JavaScript 16 MiB, API 8 MiB까지 스트리밍하며 초과 시 중단한다.
- 요청 제한은 10초, 리소스 제한은 15초다. 자산 후보는 최대 두 개다.
- 공개 토큰은 ES256/AMPWebPlay와 만료 시각을 검사하고 만료 60초 전에 폐기한다.
- 동시 토큰 요청을 합치며 서비스 종료 시 요청을 취소한다.
- 인증 실패의 재시도는 한 번이다. 공개 토큰과 사용자 토큰을 함께 갱신하고
  사용자 토큰은 ignoreCache로 다시 요청한다. 지역 캐시는 사용자 토큰에 연결한다.
- HTTP 혼잡·연결·권한 오류는 기존 Failure 분류로 전달한다. 응답 본문은 오류 문구에 넣지 않는다.

## 모델과 페이지

웹 JSON을 MusicKit의 Song/Album/Artist/Playlist/Station으로 해석해 기존 캐시에 보관한다.
재생에 필요한 PlayParameters도 이 모델에 유지한다. 라이브러리와 카탈로그의 ID는 구분한다.

저장한 플레이리스트 재생 시 캐시에 없는 카탈로그 곡 ID는 중복을 제거해 최대 300개씩
조회한다. [Apple의 복수 곡 조회 제한](https://developer.apple.com/documentation/applemusicapi/get-multiple-catalog-songs-by-id)을
따르며, AMP의 동일 경로도 300개를 허용한다. 응답 순서와 관계없이 원래 순서·중복을
복원하고, 누락이나 조회 실패는 큐 변경 전에 반환한다. 보관함 곡과 스테이션은 기존
단일 항목 조회를 사용한다.

검색과 관계 페이지는 응답 next에서 증가하는 숫자 offset만 읽는다. 응답의 호스트나 경로는
따라가지 않고 동일한 API 경로를 재구성한다. 노래가 아닌 트랙을 거를 때도 원본 offset을
유지한다. 앨범·플레이리스트 전체 재생은 모든 필요한 페이지를 읽고 2000곡 제한을 넘으면
실패한다. 일부 곡만 성공한 것처럼 반환하지 않는다.

현재 웹 계층은 GET 조회만 제공한다. 계정 플레이리스트 생성·편집과 Apple 가사 호출은
제품 기능에 포함되어 있지 않다. 가사는 [공급 경로](PROVIDERS.md)에 설명한 LRCLIB를 사용한다.
웹 경로는 비공개이므로 Apple의 변경에 따라 이 계층의 수정이 필요할 수 있다.

구현: [AppleWeb](../backend/Sources/Backend/AppleWeb.swift),
[WebMusic](../backend/Sources/Backend/WebMusic.swift),
[카탈로그 상세](../backend/Sources/Backend/CatalogDetails.swift).
검사 방법과 현재 결과는 [VALIDATION](VALIDATION.md), 실행 옵션은 [RELEASE](RELEASE.md)를 따른다.
