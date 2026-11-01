# ClipVault

[![Release](https://img.shields.io/github/v/release/ardss/clipvault)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/ardss/clipvault/ci.yml?label=CI)](../../actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ardss/clipvault)](LICENSE)

**한국어** | [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md)

강화된 Win+V — 깔끔하고 미니멀한 Windows 클립보드 기록 관리자.

`Alt+V`로 패널 호출 → 검색 → 클릭하면 원래 창으로 붙여넣기. 완전 로컬: 네트워크 제로, 계정 제로, 텔레메트리 제로.

## 기능

- **장기 기록**: 텍스트, 이미지, 파일, 리치 텍스트(웹/Word 형식), 링크 — 재부팅 후에도 유지, 보존 기간 설정 가능
- **검색**: 즉시 키워드 필터링(중국어·영어 지원)
- **고정**: 자주 쓰는 항목을 상단에. 정리 대상에서 제외
- **필터**: 전체 / 텍스트 / 이미지 / 링크 / 파일 / 고정
- **붙여넣기 복귀**: 패널은 작업 중인 창에서 포커스를 빼앗지 않습니다. 항목을 클릭하면 원래 자리에 붙습니다
- **호버 미리보기**: 잘린 텍스트와 이미지 썸네일이 옆 미리보기에서 확대(스크롤·선택·부분 복사 가능)
- **키보드 우선**: `↑↓` 이동, `Enter` 붙여넣기, `Del` 삭제, `Esc` 닫기
- **다크/라이트 테마, 7개 언어 UI**(한/영/중/일/서/불/독): 패널에서 전환, 기억됨
- **통계**: 전체 / 카테고리 / 최근 7일 추이 / 붙여넣기 Top 5
- **선택적 자동 시작**, 트레이 아이콘, 드래그로 조절되고 기억되는 패널 높이
- **캡처 일시중지**(트레이 원클릭, 자동 해제 설정 가능)와 **기록 전체 지우기**
- **프라이버시 차단, 모두 옵트인**: 민감 단어 필터(스위치 있음), 제외 앱, 이미지 캡처 토글. 비밀번호 관리자가 표시한 복사본은 기록되지 않음(Windows 표준)
- **호출 핫키 설정**(충돌 시 안전하게 재등록)
- **초대형 텍스트**(256KB 초과)는 인라인 미리보기와 함께 로컬 파일로 보관 — 조용히 유실되는 것 없음

## 프라이버시

- 100% 로컬. 데이터는 `%APPDATA%\com.clipvault.app\`에 저장(SQLite WAL + PNG 파일)
- 비밀번호 관리자의 복사본은 기록되지 않음(민감 클립보드 마커 존중)
- 사용자 정의 민감 단어 필터
- 바이너리에 네트워크 코드 없음. 자세한 내용은 [SECURITY.md](SECURITY.md)

## 설치

[Releases](../../releases)에서 인스톨러를 받거나 소스에서 빌드:

```bash
npm install
npx tauri build        # src-tauri/target/release/bundle/에 출력
```

## 빌드

Tauri 2 + Vue 3 + Rust (rusqlite / 직접 작성한 Win32 레이어).

> **주의**: 릴리스는 반드시 `npx tauri build`로. bare `cargo build --release`는
> 개발 설정이 남아 창이 vite 개발 서버에서 UI를 로드하려 합니다
> ([docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md) 참조).

개발 모드:

```bash
npx vite --port 5173        # 터미널 1
npx tauri dev               # 터미널 2
```

테스트: `cd src-tauri && cargo test` (클립보드 왕복 / DIB 디코딩 / DROPFILES 파싱)

## 테스트 및 수용

[docs/TESTING.md](docs/TESTING.md)과 [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md) 참조.

## 기술 노트

- 클립보드 쓰기 호환성: 이미지는 표준 CF_DIB(bottom-up)와 등록 형식 "PNG" 둘 다로 제공 — Chromium 계열(Chrome/VS Code/Electron)과 GDI 앱 모두 올바르게 읽음. DIB 디코딩은 BI_BITFIELDS 채널 마스크 지원(스크린샷 도구)
- 붙여넣기 키 입력은 온디맨드 인젝터 하위 프로세스가 발행(자가 복구)(일부 환경에서는 프로세스 내 합성 키가 무시됨)
- WebView IPC 하트비트 워치독: 패널이 죽은 IPC 채널을 감지해 스스로 리로드(JS 5초마다, 네이티브 폴백 60초 이내)
- 모든 데이터는 `%APPDATA%\com.clipvault.app\`에 저장

## 라이선스

[MIT](LICENSE)
