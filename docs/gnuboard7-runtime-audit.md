# GnuBoard7 프로덕션 런타임 감사

감사 기준일: 2026-07-13

## 최종 판정

- Installer Provisioning Coverage: `CONDITIONALLY_READY`
- Live Runtime Readiness: `NOT_READY`
- 한 줄 결론: 서버 프로비저닝과 G7 후속 런타임 자동화는 구현됐지만, 공식 최신 Release 7.0.3의 Vite manifest 참조 파일 누락으로 실제 앱 설치와 finalize 검증이 차단됐다.
- P0 원인: 공식 Release의 `public/build/manifest.json`이 존재하지 않는 `assets/app-UyRVujZY.js`, `assets/app-wg6V8t2K.css`를 참조한다.

설치기는 누락 파일을 자체 빌드하거나 G7 코어를 수정하지 않는다. 공식 Release가 정상화되면 같은 설치 흐름에서 다시 검증한다.

## 설치 생명주기

1. `g7inst setup`: Ubuntu 점검, 패키지, 계정과 웹루트, PHP-FPM, 웹서버, MySQL, Redis, TLS, 공식 G7 소스를 준비한다.
2. G7 공식 `/install`: 관리 계정과 G7 내부 DB 설치를 완료한다.
3. `g7inst finalize`: 공식 설치 표식을 확인한 후 SettingsService, storage link, queue, scheduler, Reverb, 실효 설정과 자산을 검증한다.
4. `g7inst reset --yes`: 설치기 소유 자원을 제거하고 인증서와 certbot 갱신 타이머는 보존한다.

## 기능별 판정

### Web/TLS

- 구현: Nginx와 Apache vhost, `public` DocumentRoot, PHP-FPM socket, canonical host, HTTP에서 HTTPS 전환, 기존 인증서 재사용과 자동 갱신을 지원한다.
- Reverb: `/app`, `/apps`를 `127.0.0.1:8080`으로 프록시하고 WebSocket Upgrade 헤더를 적용한다.
- 판정: `PARTIAL`. 서버 설정 검증은 완료했지만 최신 G7 앱 자산 문제로 최종 페이지와 자산 HTTP smoke는 미완료다.

### PHP/FPM

- 구현: PHP 8.3/8.5, 사이트 전용 pool과 socket, RAM/vCPU 기반 worker 수, OPcache, 64M 업로드, 80M POST, 120초 실행, `max_input_vars=5000`을 적용한다.
- 검증: CLI와 FPM 실효값, 필수 확장, 설정 경로를 리포트한다.
- 판정: `PASS`(프로비저닝). 실제 G7 업로드 기능은 `UNVERIFIED`다.

### Filesystem

- 구현: 사이트 계정과 `www-data` 그룹 공유, 쓰기 디렉터리, `.env` 0600, systemd `UMask=0002`, `storage:link`를 관리한다.
- 판정: `PARTIAL`. 구조와 권한 계약은 구현됐으나 앱 설치 차단으로 실제 업로드와 썸네일 생성은 확인하지 못했다.

### DB/Search

- 구현: MySQL 8.0/8.4, localhost bind, utf8mb4, 앱 DB와 최소 권한 계정, `ngram_token_size=2`, G7 `mysql-fulltext` 설정과 migrate 상태를 확인한다.
- 판정: `PARTIAL`. DB 프로비저닝은 통과했지만 실제 한글 검색 데이터 경로는 `UNVERIFIED`다.

### Redis

- 구현: localhost bind, protected mode, 메모리별 maxmemory, `volatile-lru`, PHP Redis 확장, PING과 G7 실효 cache/session/queue 드라이버를 검증한다.
- 판정: `PASS`(프로비저닝), G7 로그인 세션 지속 기능은 `UNVERIFIED`다.

### Queue

- 구현: `g7-queue.service`, 사이트 사용자, 앱 WorkingDirectory, Redis queue worker, 자동 재시작, graceful stop, `queue:restart`를 적용한다.
- 판정: `PARTIAL`. unit 문법과 서비스 상태 검증은 구현됐지만 실제 G7 Job 처리 증거는 앱 설치 차단으로 없다.

### Scheduler

- 구현: `g7-scheduler.timer`가 사이트 사용자로 매분 `schedule:run`을 실행한다. cron을 함께 추가하지 않는다.
- 검증: `schedule:list`, unit 문법, enable/active 상태를 확인한다.
- 판정: `PARTIAL`. 실제 sitemap은 검증 대상으로 포함하고, GeoIP 등 조건부 스케줄은 외부 설정에 따라 달라진다.

### Reverb

- 구현: G7 SettingsService로 외부 HTTPS/WSS와 내부 HTTP endpoint를 분리 저장하고 `g7-reverb.service`를 실행한다.
- 검증: 단순 TCP가 아니라 내부와 외부 WebSocket HTTP 101 handshake를 확인한다.
- 판정: `PARTIAL`. 자동화와 검증 코드는 완료됐지만 최신 G7 finalize가 실행되지 못했다.

### Mail

- 구현: 미사용, 로컬 Postfix, 외부 SMTP relay를 분리하고 민감값은 root 전용 파일에 저장한다. finalize가 G7 메일 설정을 공식 SettingsService로 적용한다.
- 판정: `BLOCKED_EXTERNAL`. 실제 수신, SPF, DKIM, DMARC, PTR은 계정과 DNS가 필요하다.

### Upload/Storage

- 구현: local storage와 공개 링크, PHP GD/Imagick/EXIF/fileinfo, 업로드 한도를 준비한다.
- 판정: `PARTIAL`. S3/R2는 `BLOCKED_EXTERNAL`, local 업로드와 썸네일은 앱 설치 후 기능 검증이 필요하다.

### GeoIP

- 구현: PHP maxminddb 확장과 G7 설정·스케줄 실행 기반을 준비한다.
- 판정: `BLOCKED_EXTERNAL`. MaxMind 라이선스 키와 DB는 설치기가 배포하지 않는다.

### Modules/Plugins/Templates

- 구현: finalize에서 `module:list`, `plugin:list`, `template:list`, `route:list`, `schedule:list`를 실행하고 Vite manifest의 모든 참조 파일을 검사한다.
- 판정: `PARTIAL`. 공식 7.0.3 자산 누락을 정확히 차단했으며 활성 확장 화면 smoke는 아직 실행하지 못했다.

### API/Notification/SEO

- 구현: route 목록, Reverb, queue, sitemap 생성과 `/sitemap.xml` HTTP smoke를 후속 검증한다.
- 판정: `PARTIAL`. 인증 API, private channel broadcast, 메일·database notification의 실제 기능은 앱 설치 후 검증이 필요하다.

### Security/Recovery

- 구현: DB·Redis·Reverb 내부 bind, `.env` 권한, 서비스 소유권, Certbot 보존, 설치기 소유 파일 기반 reset을 적용한다. UFW와 fail2ban은 별도 유지보수 앱 범위다.
- 초기화 대상: 사이트 계정, 웹루트, DB와 DB 계정, PHP·웹서버·DB·Redis 설정, G7 settings JSON, storage 링크, queue·scheduler·Reverb unit, 설치기 도입 패키지와 메타데이터다.
- 보존 대상: `/etc/letsencrypt` 인증서 lineage, Certbot 패키지와 `certbot.timer`다.
- 판정: `PASS`. unit·설정 파일 제거와 systemd failed 상태 정리까지 회귀 테스트와 실제 reset으로 검증했다.

## 외부 작업

G7 관리자에서 입력할 항목:

- 실제 사이트 정보, 관리자 정책, SMTP 또는 외부 저장소 자격증명
- MaxMind 키와 GeoIP 사용 여부
- 모듈·플러그인별 사업자 API와 결제·본인인증 계약 정보

설치기가 자동화하는 항목:

- OS 패키지, PHP-FPM, 웹서버, MySQL, Redis, TLS, 계정과 권한
- 공식 G7 소스 검증과 `/install` 인계
- Redis 드라이버, queue, scheduler, Reverb, storage link, 검색과 실효 설정 검증
- 설정 안내서, 리포트, 설치기 소유 자원 초기화

외부에서 처리할 항목:

- DNS와 VPS 제공자 방화벽
- SMTP/SES/Mailgun, SPF, DKIM, DMARC, PTR
- S3/R2와 MaxMind 자격증명
- 결제, 본인인증, 주소검색 등 사업자 계약

## 실제 VPS 검증

- 대상: Ubuntu 24.04, Nginx, PHP 8.5, MySQL 8.4, Redis, 기존 Let’s Encrypt 인증서
- E2E 시도: 1/5
- 서버 프로비저닝: TLS 단계까지 통과
- 소요시간: 1분 49초
- 앱 소스: 공식 G7 7.0.3 commit `a34a03eb40451fff271b1e244d71d72e307ee1ef`
- 차단 결과: Git clone과 repository 검증은 통과했으나 Vite manifest 참조 자산 2개가 공식 소스에 없어 앱 설치 전에 중단
- 반복하지 않은 이유: 같은 tag와 commit의 결정적 외부 결함이므로 재시도해도 결과가 동일하다.
- 초기화: 후속 runtime 설정과 서비스까지 제거되고 인증서 내용은 보존됨을 확인

## 최종 체크리스트

- [ ] 브라우저 G7 설치 완료
- [ ] 실효 드라이버 확인
- [ ] Redis cache/session 실제 로그인 검증
- [ ] async queue Job 처리
- [ ] scheduler 매분 실행 journal
- [ ] Reverb 외부 WSS와 broadcast 송신
- [ ] 비밀번호 재설정 메일 수신
- [ ] local 파일 업로드와 이미지 썸네일
- [ ] FULLTEXT 한글 검색
- [ ] sitemap/GeoIP 조건부 schedule
- [ ] 모듈·플러그인·템플릿 asset smoke
- [x] 재부팅 자동기동 unit 구성
- [x] report.json finalize 상태 계약
- [x] reset 소유 자원과 인증서 보존 계약

미완료 체크 항목은 공식 G7 배포 자산이 정상화된 뒤 브라우저 설치와 finalize를 실행해야 닫을 수 있다.
