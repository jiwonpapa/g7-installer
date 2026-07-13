# GnuBoard7 프로덕션 런타임 감사

감사 기준일: 2026-07-13

## 최종 판정

- Installer Provisioning Coverage: `READY`
- Live Runtime Readiness: `CONDITIONALLY_READY`
- 한 줄 결론: 공식 G7 7.0.3의 브라우저 설치와 핵심 로컬 런타임은 실제 VPS에서 통과했다. 외부 SMTP, S3/R2, GeoIP, 결제·본인인증은 별도 자격증명이 필요하다.
- Upstream 경고: 공식 `public/build/manifest.json` 참조 이름과 동봉 자산 이름이 다르고 메인 화면 fallback `/build/assets/app.css`는 404다. 설치기는 G7 코어를 수정하거나 `npm run build`를 실행하지 않고 리포트에 경고한다.

## 설치 생명주기

1. `g7inst setup`: Ubuntu 점검, 패키지, 계정·웹루트, PHP-FPM, 웹서버, MySQL, Redis, TLS와 공식 G7 소스를 준비한다.
2. G7 공식 `/install`: 관리자 계정과 G7 내부 DB 설치를 완료한다.
3. `g7inst finalize`: SettingsService, storage link, Queue, Scheduler, Reverb, 실효 설정과 HTTP 자산을 검증한다.
4. `g7inst reset --yes`: 설치기 소유 자원을 제거하고 인증서와 Certbot 갱신 타이머는 보존한다.

## 기능별 판정

| 영역 | 자동화 및 실제 검증 | 판정 |
| --- | --- | --- |
| Web/TLS | Nginx/Apache vhost, `public` DocumentRoot, HTTPS 정규화, apex+www 인증서, Reverb WebSocket proxy | `PASS` |
| PHP/FPM | PHP 8.3/8.5, 사이트 pool, 실효 설정, 필수 확장, GD·Imagick 원본/썸네일 생성 | `PASS` |
| Filesystem | 사이트 계정, 공유 권한, `.env` 0600, storage link, 공개 이미지 HTTP 200 | `PASS` |
| DB/Search | MySQL 8.0/8.4, utf8mb4, 최소 권한 계정, 113개 table, 15개 FULLTEXT index, ngram 2 | `PASS` |
| Redis | localhost/protected mode, PING, cache·session·queue 실효 driver와 실제 session key 생성 | `PASS` |
| Queue | `g7-queue.service`, restart 정책, 임시 Job enqueue와 worker 처리 round-trip | `PASS` |
| Scheduler | systemd timer, `schedule:list`, sitemap 생성과 HTTP smoke | `PASS` |
| Reverb | localhost listener, 내·외부 WebSocket 101, backend broadcast publish | `PASS` |
| Modules/Plugins/Templates | 공식 설치 화면에서 3개 module, 6개 plugin, 2개 template와 12개 언어팩 설치 | `PASS` |
| Public assets | 같은 도메인 JS/CSS 10개 중 9개 HTTP 통과, upstream fallback CSS 1개 404 | `WARN_UPSTREAM` |
| Mail | 미사용·Postfix·외부 relay 설정 기반 제공, 실제 송수신과 DNS 정책은 외부 조건 | `BLOCKED_EXTERNAL` |
| S3/GeoIP/사업자 API | 런타임 기반과 확장 설치, 실제 자격증명과 계약은 사용자가 제공 | `BLOCKED_EXTERNAL` |

## 외부 작업

- DNS와 VPS 제공자 방화벽
- SMTP/SES/Mailgun, SPF, DKIM, DMARC, PTR
- S3/R2와 MaxMind 자격증명
- 결제, 본인인증, 주소검색 등 사업자 계약

UFW와 fail2ban은 별도 유지보수 앱 범위이며 설치기가 변경하지 않는다.

## 실제 VPS 검증

- 대상: Ubuntu 24.04.4, Nginx, PHP 8.5.8, MySQL 8.4.10, Redis, 기존 Let’s Encrypt 인증서
- E2E 시도: 3/5. 1차는 upstream manifest를 차단 오류로 분류하던 정책을 발견했고, 2차와 `beta.10` 최종 3차는 공식 웹 설치와 finalize를 완료했다.
- E2E: 공식 G7 7.0.3 commit `a34a03eb40451fff271b1e244d71d72e307ee1ef` 브라우저 설치 완료
- 최종 프로비저닝 소요시간: 1분 39초
- 서비스: Nginx, PHP-FPM, MySQL, Redis, Queue, Scheduler, Reverb active/enabled
- 네트워크: 80/443/22만 외부 bind, MySQL·Redis·Reverb는 `127.0.0.1` bind
- 보안: `.env` 0600, `/install/` 410, `.env`와 `.git/config` HTTP 403
- 런타임: Redis session key, Queue Job 처리, Reverb broadcast, sitemap, 이미지 썸네일, FULLTEXT 구성을 확인

## 초기화 검증

실제 웹 초기화와 회귀 테스트에서 다음 자원을 제거했다.

- 사이트 계정과 웹루트, DB와 DB 계정, 설치기 도입 패키지
- PHP·웹서버·MySQL·Redis 설정과 데이터 디렉터리
- G7 `drivers.json`, `mail.json`, `public/storage` 링크
- `g7-queue.service`, `g7-scheduler.service`, `g7-scheduler.timer`, `g7-reverb.service`
- 설치기 state, report, owned-files, rollback, backup 메타데이터

보존 확인:

- `/etc/letsencrypt` 인증서 lineage와 인증서 파일 SHA-256
- Certbot 패키지와 active/enabled `certbot.timer`

초기화 후 fresh doctor는 실패 0건으로 신규 설치 가능 상태를 확인했다.

## 최종 체크리스트

- [x] 공식 G7 브라우저 설치
- [x] Redis cache/session/queue 실효값과 실제 동작
- [x] Queue Job round-trip
- [x] Scheduler와 sitemap
- [x] Reverb WebSocket과 backend publish
- [x] local 이미지 생성·썸네일·공개 URL
- [x] MySQL FULLTEXT/UTF8MB4/ngram 구성
- [x] 재부팅 자동기동 unit과 finalize report 계약
- [x] 추가 settings·storage·runtime unit 초기화와 인증서 보존
- [ ] 외부 메일 송수신, S3/R2, GeoIP, 결제·본인인증
- [ ] G7 upstream fallback CSS 404 해소
