# DG-1 — macOS 개발 적용과 자기 적용

소유 저장소: DevGuard. 현재 구현 상태: `in-progress` / qualification `not-run`. 진입: DG-0 정확한 source의 계약 검증. 종료: 실제 macOS 실행·회수, generic/Cargo 소비, 상위 예산 안의 후보 시험, 독립 복구, 개발·foreground SLO를 통과한 artifact/정책/환경 조합 확보.

아래 ID·제목·PR 묶음은 **예정 값**이다. 실제 SHA나 GitHub PR 번호가 아니다. 모듈 경로는 구현 전까지 예정 책임을 나타낸다. 현재 daemon/client crate는 존재하며 launcher·platform-macos·cli·adapters는 후속 책임이다. crate 추가는 해당 PR에서 명시적 의존 allowlist와 전체 그래프 검증을 함께 확장하고 검사를 제거하지 않는다. 실제 상태는 ledger가 소유한다.

구현된 C01은 정상 경로·명시 bootstrap·배타 journal 검사를 제공한다. C02는 foreground devguardd serve, 제한된 인증 UDS 통신, OS UID/PID 관측, 엄격한 client 호환성과 private 자격 FD 전달을 추가한다. C03은 `devguard-macos`의 boot 시계, native PID/start 정체성, 호스트 용량과 serve에서 journal을 활성화하는 2초 압력 sampler를 추가한다. C04는 협조적 QoS/nice 적용과 readback, 이탈·추적 상실이 고정되는 관측 process group scope, 정체성을 확인한 종료를 추가한다. C05는 wire 등록과 fenced `devguard-launch` helper를 추가한다. Grant마다 claim된 helper는 하나이며, READY와 exec 전에 scope binding과 authorization을 거치고, transcript와 exec 실패 보고를 분리하며, payload descriptor를 정리한다. C06은 서비스 reconciler를 추가한다. 관측된 scope 종료, helper가 없다는 owner 보고, 이전 boot에서만 회수하며, reap 전 owner 관측, scope 종료, instance 폐기, 재시작 전후의 Suspect 회계를 제공한다. Native 증거가 있으면 서비스는 등록·launch·대조를 연다. C07은 `devguard` 명령행 owner를 추가한다. 명시적이고 제한된 대기, terminal·signal 전달, reap 전 관측, receipt, doctor 진단을 갖춘 관리 실행을 제공하며, 관리되지 않는 대체 실행은 없다. C08은 Cargo adapter를 추가한다. Compiler job을 예약에 맞춰 조정하거나 거절하며, pipeline과 중첩 Cargo 실행이 jobserver 하나를 공유하게 한다. C09는 패키지로 만든 release를 현재 사용자 LaunchAgent로 설치한다. hash와 컴파일된 호환성을 담은 manifest, 변경 불가능한 release·복구 사본을 두며, launchd가 그 release의 바이너리를 실행한다고 검증한 뒤에만 선택한다. C10은 부모 lease와 후보 authority를 추가한다. Lease는 호스트에 한 번 과금되고, 자식은 lease의 남은 예산에 대해서만 admission되며 lease가 끝나면 fence된다. 후보 authority는 lease로 제한되며 launch 없이 admission만 한다. `devguard test-candidate`는 후보 tree의 build·시험·authority를 lease 하나의 자식으로 실행한다. C11은 upgrade와 repair를 추가한다. 관리자 drain은 admission을 닫으며 재시작 뒤에도 유지된다. Staged release는 drain, quiescent 백업, 닫힌 채 시작한 새 release의 검증을 거친 뒤에만 현재 release를 교체한다. 새 release가 실패하면 이전 release를 같은 journal로 다시 시작한다. 호환되지 않는 downgrade는 거절한다. Repair는 last known good release나 그 복구 사본으로 복구하며 두 번째 authority를 시작하지 않는다. C12는 SLO 판정 harness를 추가한다.
- 자기 target을 소유하고 상태와 종료 확인을 재는 제어 probe
- `devguard exec`로 실행하는 제한된 작업 부하
- 전경 브라우저 fixture
- `scripts/measure.py`: 설치된 release에 대해 protocol을 실행하고, 구간·반복·조합을 판정하며, qualified release만 승격한다.

PR과 병합 후 main 전달 증거는 별도로 추적한다. [운영 문서](../../operations.md)를 참조한다.

## PR 순서와 활성화 경계

| 예정 PR | 작업 | 선행 PR | 함께 제공할 경계 |
| --- | --- | --- | --- |
| DG1-P1 | DG1-C01, DG1-C02 | DG-0 | 정상 authority 경로와 인증; 실행 capability는 아직 닫힘 |
| DG1-P2 | DG1-C03, DG1-C04 | DG1-P1 | probe와 적용·종료 증거; 관측 없이 성공 금지 |
| DG1-P3 | DG1-C05, DG1-C06 | DG1-P2 | launch와 모든 정리/불확실 경로를 함께 검토 |
| DG1-P4 | DG1-C07, DG1-C08 | DG1-P3 | 실제 개발 진입점과 도구 의미 보존 |
| DG1-P5 | DG1-C09, DG1-C10, DG1-C11 | DG1-P4 | 설치·후보·독립 복구를 묶어 자기 적용 활성화 |
| DG1-P6 | DG1-C12 | DG1-P5 | 기능 기준 artifact와 SLO 안정 artifact를 구분해 승격 |

공통 현재 명령 python3 scripts/validate.py --offline은 Rust 1.95.0 계약 회귀를 확인하며 fake backend로 native 동작을 입증하지 않는다. C01 authority, C02 인증·transport, C03 native probe, C04 native scope, C05 native launch, C06 native 대조, C07 CLI, C08 Cargo, C09 설치, C10 부모 lease, C11 upgrade, C12 SLO harness suite는 현재 제공한다. 아래에서 예정이라고 명시한 나머지 명령은 미제공이며 각 PR에서 fixture·실행 case 수·log·정리를 함께 구현하고 제공 상태를 갱신한다. 이름만 있는 테스트나 0개 실행을 통과로 처리하지 않는다. 공통 toolchain·증거·SLO 규칙은 상위 검증 문서에 있다.

P1은 runtime을 닫아 두고 P2는 실제 probe, P3는 launch·안전 정리, P4는 개발 진입점, P5는 설치·부모 예산·repair, P6는 측정·승격을 제공한다. C08까지 foreground daemon과 최소 단일 Cargo job·test thread bootstrap을 사용한다. P4 bundle은 삭제할 build 경로 밖에 보존한다. C10에서 부모 예산을 포함한 artifact를 먼저 기능 시험·동결한 직후 제한된 실제 자기 적용을 시작하며 SLO qualification은 C12에서 확립한다.

### DG1-C01 — 운영 설정과 authority 경로

- 소유/예정 PR: DevGuard / DG1-P1. 예정 제목: `feat(daemon): define canonical authority and bounded test configuration`.
- 문제 → 동작: DG-0 디렉터리 잠금을 서비스 하나의 정상 경로·권한·설정 검증으로 감싸 경로 변경에 의한 이중 호스트 예산을 거절한다.
- 선행: DG-0. 운영 조건: 실제 실행 호스트, 설정된 서비스 UID와 보호된 state/socket 부모. 시험 authority는 부모 증빙 없으면 시작 불가.
- 대상/산출물: 예정 daemon 설정 loader·경로 결정기·doctor 진단, 기존 `Authority::open` 경계와 설정 예시. symlink/소유권 검사를 포함한다.
- 불변 조건: core 예산 산식·기존 journal init/open 분리·정적 예약·실행 capability closed. 설정 변경이 live consumer를 새 슬롯으로 취급하지 않는다.
- 시험: 정상 단일 시작; 잘못된 권한/경로 별칭/누락 journal 거절; 동시 두 프로세스와 다른 socket 이름이 하나의 정상 authority만 얻는 경쟁.
- 검증 명령: 현재 공통 회귀 + 제공되는 `python3 scripts/qualify.py dg1-authority --offline`. 설정·저장소 검증이며 native 제어 자격은 아니다.
- 완료 증거: 경로·UID·lock 소유 관측, 중복 시작 거절 로그, 설정 fingerprint. credentials와 전체 개인 경로 로그는 정제한다.
- rollback: 서비스 시작을 중지하고 기존 journal을 보존; live instance 설정을 임의 축소하지 않는다.
- 인계: DG1-C02에 정상 transport endpoint와 관리/시험 모드 판별을 전달. C02와 함께 PR을 제출한다.

### DG1-C02 — UDS 인증과 자격 전달

- 소유/예정 PR: DevGuard / DG1-P1. 예정 제목: `feat(client): authenticate local peers and transfer scoped credentials`.
- 문제 → 동작: 호출자가 선언한 PID/UID 대신 OS peer 관측을 사용하고 version/capability를 검증한 client protocol을 제공한다.
- 선행: DG1-C01. 실제 UDS peer 확인 가능 환경, consumer generation·secret 설치 경로. 아직 사용자 명령 실행은 제공하지 않는다.
- 대상/산출물: daemon/client framing·foreground 서비스·private credential FD API·handshake와 strict-decoding fixture. C02는 peer UID/PID를 관측한다. Daemon은 C03이 Backend를 통해 boot/start 증거를 제공한 뒤에만 TrustedPeer를 native 등록에 연결하며 TrustedPeer 자체에는 UID/PID만 있다.
- 불변 조건: payload에 peer identity/관리 Principal 선언 불가; UID만으로 역할 획득 불가; 모든 consumer·관리 digest 분리; helper permit을 caller 자격으로 사용 금지. Secret을 argv/env/journal/debug에 남기지 않고 payload 64 KiB·세션 32개·idle 대기를 포함한 frame별 절대 250 ms 기한을 지킨다. 인증은 동일 UID 공격자를 격리하거나 Principal·lease를 발급하지 않는다.
- 시험: 정상 인증·status·재접속; 잘못된 UID/PID·credential·generation·wire/capability 거절; 동시 등록 요청은 모두 닫힌 상태 유지; 부분·느린·마지막 frame과 포화·후속 exec 전 private FD 닫기. C05 helper qualification은 아니다.
- 검증 명령: 현재 공통 회귀 + 현재 제공되는 python3 scripts/qualify.py dg1-auth --offline. Transport·저장소 동작을 입증하며 native 자원·SLO 자격은 부여하지 않는다.
- 완료 증거: 양 끝에서 대조한 OS peer 관측, 권한 오류 표, 필드 추가 비호환성을 포함한 구신 decoding 결과. Native 등록은 not_run이다. 설정 schema 1에서 이전 시스템 task 예약 16은 거절하고 최소 48(session 32+서비스/관제 여유 16)을 요구한다. 자동 migration·kernel 제한 주장이 아닌 명시적 운영자 용량 검토를 문서화한다.
- rollback: 신규 연결/admission을 닫고 기존 lease를 유지; 자격 회전은 live generation 대조 이후 수행한다.
- 인계: DG1-C03~C06에 OS 인증 세션·private FD API를 전달하고 C03이 boot/start 정체성을 제공한 뒤 native Principal을 생성한다. 인증과 canonical 경로를 분리 활성화하거나 통신 장애로 실행·회수를 추정하지 않는다.

### DG1-C03 — 호스트 probe와 프로세스 정체성

- 소유/예정 PR: DevGuard / DG1-P2. 예정 제목: `feat(macos): observe boot identity and host pressure`.
- 문제 → 동작: 가짜 clock/PID/압력 대신 실제 boot-relative clock, PID start identity, CPU·메모리·disk 압력 관측을 제공한다.
- 선행: DG1-C02. 지원 macOS 버전·권한을 명시하고 측정 실패를 보고할 수 있어야 한다.
- 대상/산출물: 예정 platform-macos `Backend`와 probe adapter, `pressure.rs` 연결, 호스트 정보 receipt.
- 불변 조건: 첫 유효 sample 전 closed; stale/future sample 거절; 6초 freshness·30초 회복 계약 유지; 관측 실패와 미지원 capability 분리.
- 시험: 정상 부하·회복; probe 오류/지연/boot 변경; PID 재사용과 sample replay 경쟁; sampler 지연 중 admission fail-closed.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-probes --offline`(macOS 전용; 다른 플랫폼은 `not_run` 기록). 등록을 닫은 상태의 native boot·정체성·압력 증거를 검사하며 자원 정책 적용은 검사하지 않는다.
- 완료 증거: 원시 probe 표본·clock 단위·identity 재현 기록, 장애 주입과 closed 전환 시간.
- rollback: probe 오류 시 신규 작업 차단, 기존 lease 축소 금지; 가짜 값으로 fallback하지 않는다.
- 인계: DG1-C04에 freshness가 검증된 identity·압력 입력. P2에서 실제 scope 증거와 함께 검토한다.

### DG1-C04 — macOS scope와 적용·종료 증거

- 소유/예정 PR: DevGuard / DG1-P2. 예정 제목: `feat(macos): verify resource policy and scope termination`.
- 문제 → 동작: plan을 실제 적용 결과로 오인하지 않도록 자원별 method/level과 추적 가능한 scope 증거를 반환한다.
- 선행: DG1-C03. 지원되는 QoS/priority 및 관측 권한; tree-wide hard memory cap을 지원한다고 가정하지 않는다.
- 대상/산출물: 예정 scope tracker와 apply/probe adapter, 기존 binding/reconciliation evidence 연결; 실패 수준별 capability 표.
- 불변 조건: 실제 readback/관측 전 applied 성공 금지; root reap만으로 회수 금지; 추적 상실 sticky; PID 재사용 방지.
- 시험: 정상 자손 종료·정책 적용; 권한 거절·지원 불가·관측 누락; root 종료 뒤 자손 생존, 자손 이동/추적 상실과 회수 경쟁.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-scopes --offline`(macOS 전용; 다른 플랫폼은 `not_run` 기록). 라이브러리를 통해 실제 scope root를 검사하며 서비스는 DG1-P3 전까지 scope를 수립하지 않는다.
- 완료 증거: 자원별 requested/supported/applied 및 정확한 scope 종료 증거. kernel 요구 거절 결과 포함.
- rollback: 적용 실패 scope는 실행 허용 전에 정리; 추적 불확실이면 Suspect와 회계를 보존한다.
- 인계: DG1-C05가 실행을 허용하기 전 사용할 실제 binding witness와 회수 관측 API.

### DG1-C05 — launch helper의 준비와 실행 허용

- 소유/예정 PR: DevGuard / DG1-P3. 예정 제목: `feat(launcher): fence helper preparation and executable start`.
- 문제 → 동작: durable launch grant 한 번으로 helper를 만들고 bind/apply/authorize/READY를 거쳐 사용자 executable로 전환한다.
- 선행: DG1-C04. 인증 client와 실제 scope 생성·적용, private credential FD. C06 완료 전 일반 실행 capability를 활성화하지 않는다.
- 대상/산출물: 예정 launcher, pipe/PTY에 연결할 private API, launch transcript와 exec failure 채널.
- 불변 조건: `begin_launch` replay는 새 permit 없음; `may_exec` 응답 유실은 불확실; READY·RunAuthorized·executable 성공을 구분; payload 전에 자격 FD 닫기.
- 시험: 정상 argv 실행과 종료값; helper 생성 전 실패/READY 전 실패/READY 후 executable 실패; permit 응답 유실·중복 helper·지연 helper 경쟁.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-launch --offline`(macOS 전용; 다른 플랫폼은 `not_run` 기록). 합성 정상 probe를 쓰는 격리 시험 authority로 실제 helper를 실행한다.
- 완료 증거: attempt별 helper 생성 수 0/1, 단계별 관측 기록, payload FD 목록과 secret 비노출 결과.
- rollback: 신규 launch를 fence하고 살아 있는 helper/scope를 C06 경로로 대조; 무조건 lease 반환 금지.
- 인계: DG1-C06에 committed/unknown 분기와 취소 hook; 두 작업을 같은 PR에서 리뷰한다.

### DG1-C06 — 취소·만료·불확실 실행의 대조

- 소유/예정 PR: DevGuard / DG1-P3. 예정 제목: `feat(launcher): reconcile cancellation expiry and uncertain execution`.
- 문제 → 동작: 취소 응답만으로 용량을 재할당하지 않고 Prepared와 committed 실행의 수명을 다르게 처리한다.
- 선행: DG1-C05. 실제 helper/scope 관측과 journal; 중단 후 재시작 가능한 격리 fixture.
- 대상/산출물: 예정 daemon reconciler·helper cancel/stop, 기존 `known_not_started`/release_reason 매핑, 장애 대응 절차.
- 불변 조건: Prepared 5초 원래 deadline; committed는 단순 TTL 해제 금지; root/owner 소실은 종료 증거 아님; tombstone 유지.
- 시험: 준비 취소/만료 정상; journal write 실패·daemon crash·helper 무응답; commit/cancel 경계, 늦은 bind, 재시작 직후 회수 경쟁.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-reconcile --offline`(macOS 전용; 다른 플랫폼은 `not_run` 기록). 자식 프로세스의 daemon crash와 재시작을 포함한다.
- 완료 증거: 중복 실행·조기 회수 0, 종료/NoHelperCreated/이전 boot 증거 구분, 재시작 ledger 합계 불변.
- rollback: admission 중지 뒤 기존 버전 reconciler의 지원 범위로 drain; 미지원 schema면 in-place downgrade 금지.
- 인계: DG1-C07에 명시적 거절·미시작·불확실 receipt와 안전한 종료 API를 제공한다.

### DG1-C07 — generic CLI와 진단·대기

- 소유/예정 PR: DevGuard / DG1-P4. 예정 제목: `feat(cli): govern commands with doctor receipts and explicit waits`.
- 문제 → 동작: 설정 파일 존재에 그치지 않고 실제 명령이 정상 authority의 admission을 소비하는 진입점을 제공한다.
- 선행: DG1-C06. real daemon/helper, 명시 consumer·정책, 충분한 예산. 기본 무한 대기는 허용하지 않는다.
- 대상/산출물: 예정 CLI `exec`/`doctor`/receipt·명시적 wait 옵션, signal/exit 전달과 generic argv 계약. 구체 옵션은 구현 PR에서 확정한다.
- 불변 조건: command 의미·cwd·env·종료값 보존; shell 암묵 삽입 금지; 기존 상태/종료는 새 admission과 독립; 최소 budget 미충족 거절.
- 시험: 정상 argv/exit/signal; daemon unavailable·capability mismatch·예산 거절; wait 취소와 grant 응답 유실 경쟁.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-cli --offline` (macOS 전용이며 다른 플랫폼은 `not_run`으로 기록한다). 먼저 `devguard-launch`를 빌드한다.
- 완료 증거: command→attempt→lease→scope 대응 receipt, 대기 deadline·취소 결과, 미관리 직접 실행과 구분되는 진단.
- rollback: 신규 CLI 사용을 중지하고 기존 관리 명령을 drain; 자동 무관리 실행 fallback은 제공하지 않는다.
- 인계: DG1-C08에 argv transformer와 중앙 admission을 분리한 adapter API를 제공한다.

### DG1-C08 — Cargo와 pipeline·jobserver

- 소유/예정 PR: DevGuard / DG1-P4. 예정 제목: `feat(cargo): preserve pipeline semantics and shared jobserver budgets`.
- 문제 → 동작: 직접 Cargo와 기존 validation pipeline을 같은 lease 안에서 실행하며 병렬도 풀을 중복 생성하지 않는다.
- 선행: DG1-C07. 명시 adapter 선택, Cargo toolchain 확인, inherited jobserver의 소유·FD 확인.
- 대상/산출물: 예정 Cargo adapter, pipeline wrapper, jobs/환경 조정 정책; CodeSpace 검증 entrypoint를 수정 없이 감싸는 예시.
- 불변 조건: 기존 `CARGO_TARGET_DIR`와 `target/upstream-reports/local` 경로 보존; explicit 옵션 충돌은 설명·거절; 자식 jobserver token 중복 발행 금지.
- 시험: 정상 cargo build/test와 pipeline; 미지원 subcommand/충돌 jobs·닫힌 FD; nested Cargo·동시 소비자·cancel 중 token 회수.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-cargo --offline` (macOS 전용이며 다른 플랫폼은 `not_run`으로 기록한다). 작업 용량이 Cargo job 하나도 수용하지 못하는 호스트는 build 경우를 `not_run`으로 기록한다. CodeSpace 실제 검증은 CS-RG qualification과 별개다.
- 완료 증거: 원래/변환 argv·env 차이, 동일 종료값/산출 경로, parent budget 이하의 관측 병렬도와 FD 누출 0.
- rollback: adapter 경로를 비활성화하고 검증된 generic 소비로 복귀; 기존 target/cache 삭제 금지.
- 인계: DG1-C09에서 사용할 제한된 기능 기준 artifact를 이 기능·장애 suite 통과 후 동결한다.

### DG1-C09 — artifact 식별과 최초 설치

- 소유/예정 PR: DevGuard / DG1-P5. 예정 제목: `feat(install): identify artifacts and establish a bootstrap reference`.
- 문제 → 동작: 소스 빌드 성공과 실제 설치 artifact를 구분하고 최초 bootstrap을 재현 가능한 제한된 기준으로 동결한다.
- 선행: DG1-C08. C01~C08 기능·장애 시험 통과, 운영자 지정 설치 경로와 제어 예약; 제품 SLO 합격은 아직 요구하지 않는다.
- 대상/산출물: artifact manifest/hash·daemon/helper 호환 정보, 예정 installer/service 시작·상태 확인, immutable 복구 사본.
- 불변 조건: 후보가 기준 artifact 덮어쓰기 금지; bootstrap 기준은 기능 시험만 통과한 상태로 표시; 정상 authority는 하나.
- 시험: 정상 최초 설치/재시작; hash mismatch·부분 설치·지원되지 않는 host; 동시 시작/설치 경쟁.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-bootstrap --offline` (macOS 전용이며 다른 플랫폼은 `not_run`으로 기록한다). launchd gui domain이 없는 session은 launchd 경우를 `not_run`으로 기록한다.
- 완료 증거: 실제 실행 binary hash와 manifest 일치, 서비스 PID/endpoint, 기능 기준 artifact의 한정된 검증 범위.
- rollback: 서비스 시작 전 검증 실패 시 이전 보존 사본 선택; 활성 journal을 과거 snapshot으로 교체하지 않는다.
- 인계: DG1-C10에 보호된 설치·복구 artifact를 제공한다. C09 artifact가 새 C10 상위 lease 기능을 이미 지원한다고 가정하지 않는다. P5 전체 전 일상 자기 적용으로 홍보하지 않는다.

### DG1-C10 — 상위 예산 안의 후보 시험

- 소유/예정 PR: DevGuard / DG1-P5. 예정 제목: `feat(self-use): constrain candidate authorities within parent leases`.
- 문제 → 동작: DevGuard 자체 개발 후보가 자기 예산을 전체 호스트 용량으로 다시 발행하는 것을 막는다.
- 선행: DG1-C09. C10 부모 예산 기능을 먼저 시험한 뒤 이를 포함하여 동결한 부모 artifact, 유효 상위 lease, 격리된 state/socket/credential/cache, 부모의 실행 fence.
- 대상/산출물: 예정 bounded-test mode, 부모 capability 검증, 후보/자식 workload 합산과 fixture runner.
- 불변 조건: 후보 CPU·memory·tasks 합계는 상위 budget 이하; 정상 journal 접근 불가; parent 소실/만료 시 신규 grant 금지.
- 시험: 정상 후보 build/test; 초과 요청·가짜 parent·후보 정책 오류; 후보 crash와 부모 cancel·늦은 자식 시작 경쟁.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-self-use --offline` (macOS 전용이며 다른 플랫폼은 `not_run`으로 기록한다). 격리 fixture 부모로 실행하며, 동결한 설치 부모 아래의 실제 자기 적용은 완료 증거로 따로 기록한다.
- 완료 증거: 상위/하위 attempt 관계와 합계, 두 번째 full-host authority 거절, 실제 자식 scope 관측과 기준 artifact 보존.
- rollback: 부모가 후보 scope를 정리·대조; 후보 자신의 admission이나 정상 journal 재초기화에 의존하지 않는다.
- 인계: C10 기능 checkpoint 직후 실제 후보를 부모로 실행하고 이후 해당 build/test를 부모로 관리하며 admission/launch/대조 receipt를 보존한다. DG1-C11에 부모 경유 drain·독립 repair와 실패 증거를 전달한다.

### DG1-C11 — drain·upgrade·repair·rollback

- 소유/예정 PR: DevGuard / DG1-P5. 예정 제목: `feat(operations): recover upgrades without candidate admission`.
- 문제 → 동작: 고장 난 후보가 복구 작업 자체를 거절하거나 오래된 journal 복원으로 중복 admission을 만드는 상황을 방지한다.
- 선행: DG1-C10. 보존된 기준 artifact, 독립 운영자 진입점·제어 여유, 실제 N/N+1 fixture와 schema 정책.
- 대상/산출물: 예정 drain/upgrade/repair 절차와 도구, client/artifact/schema 호환 표·downgrade 제한·운영 runbook.
- 불변 조건: 새 admission은 닫아도 조회/종료/대조는 가능; 새 쓰기 이후 과거 DB snapshot 복원 금지; 엄격한 serde에 필드 추가만으로 호환 주장 금지.
- 시험: 정상 N→N+1; 설치/정책/journal 실패·drain timeout; 구·신 client 동시 요청, upgrade 중 응답 유실과 late helper.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-upgrade --offline` (macOS 전용이며 다른 플랫폼은 `not_run`으로 기록한다). Fake service manager로 fixture release를 교체하며, 설치된 서비스의 실제 upgrade는 완료 증거로 따로 기록한다.
- 완료 증거: 후보 불능 상태에서 복구 완료, 회계·tombstone 보존, unsupported downgrade 거절, runtime artifact 식별.
- rollback: 호환 artifact로만 복귀하며 incompatible journal은 지원 reader로 drain/forward repair; 작업 재실행 금지.
- 인계: DG1-C12에 설치·자기 적용·복구가 포함된 기능 시험 완료 조합을 전달한다.

### DG1-C12 — 개발·자기 적용 qualification

- 소유/예정 PR: DevGuard / DG1-P6. 예정 제목: `test(qualification): qualify macOS development and bounded self-use`.
- 문제 → 동작: 기능 시험 성공을 일상 개발 응답성 성공으로 오인하지 않도록 측정된 안정 artifact를 식별한다.
- 선행: DG1-C11. 유효 foreground fixture·고정 workload·충분한 host 예산, 기능 기준 artifact와 원시값 보존 공간.
- 대상/산출물: 예정 macOS qualification harness·3회 결과·안정 artifact manifest와 지원 환경표; DG1 capability 문서.
- 불변 조건: idle 10분/부하 최소 30분/3회, cold/warm 분리; DG-1 종료가 CS-RG 구현에 의존하지 않음. 미구현 MCP 결합 SLO는 여기서 합격시키지 않는다.
- 시험: generic/Cargo 동시 소비 정상; daemon/probe/후보 실패; 부하 중 취소·재시작·late helper; foreground와 standalone 조회/종료 지연 측정.
- 검증 명령: 현재 공통 회귀 + **제공** `python3 scripts/qualify.py dg1-macos --offline`. macOS 전용이며 다른 플랫폼은 `not_run`으로 기록한다. SLO가 아니라 harness를 검사한다. SLO protocol 자체는 **제공** `python3 scripts/measure.py macos --release ID ...`이다. 따로 비워 둔 시간에 대상 호스트의 설치 release에 대해 실행하며, 그 판정과 승격은 완료 증거로 기록한다. 실제 CodeSpace 결합은 CSRG-C08에서 수행한다.
- 완료 증거: source/artifact/policy/host 조합, 유효 baseline, 모든 원시 표본·p99·실패/거절/peak/처리량, 자기 적용 제한과 통과 범위.
- rollback: 기준 artifact로 신규 개발 진입을 전환하고 실패한 조합 승격을 취소; 보존된 증거는 삭제하지 않는다.
- 인계: CSRG-C01은 이 qualification 조합에서 pin 후보를 선정한다. DG-CACHE/DG-ADAPTERS도 이 결과 이후 시작하며 구현 완료와 플랫폼 자격은 별도 기록한다.

## 승인된 실제 실행 조건

P1~P4는 foreground daemon, P5부터 현재 사용자 LaunchAgent를 사용한다. 단일 Cargo job/시험 thread bootstrap을 기록하고 P4 정리 전에 기능 bundle을 target 밖에 보존한다. C10에서 검증·동결한 부모로 즉시 bounded 실제 자기 적용을 시작하되 SLO 승격은 C12다. C12는 현재 8논리CPU/16GiB macOS에서 조합별 idle10분+최소30분 부하를3회 실행하고 전체 구간 visibility/focus를 확인한다. 무효 조건은 inconclusive다. macOS NOTE_TRACK은 미지원이므로 사용하지 않고 협조적 group 관측의 범위를 명시한다.
