# P1-RECOVERY — 독립 Runner 생존 중 Gateway 복구

> *2026-10-14 현재 상태:* 소유자는 2026-10-14에 P1-RECOVERY 구현을 CodeSpace에서 P1R-C01부터 P1R-C06까지 단위마다 PR 하나로 차례로 진행하도록 지시했습니다. 각 단위는 앞 단위가 병합·검증된 뒤 소유자의 승인으로 시작합니다. P1R-C01은 병합·검증되었고(novelKR/CodeSpace#102, CodeSpace `main` `feb5f52`), 지금 진행하는 단위는 P1R-C02입니다. 원장은 P1-RECOVERY를 `in-progress`, qualification `not-run`으로 기록하고, 새 모드는 P1R-C06의 qualification 전까지 지원 모드가 아닙니다. 아래 [재기준](#재기준-2026-10-14)이 이 계획의 나머지와 다르면 재기준을 따르며, 나머지는 이력으로 바꾸지 않고 남깁니다.

소유 저장소: CodeSpace. 상태: `not-started` / `not-run`. 진입: CSRG-C08. 완료: 운영자 선택 모드에서 독립 Runner가 기존 프로세스·입출력·timeout을 계속 소유하고 Gateway 반복 재시작과 재연결을 복구한다. InProcess 및 Runner 자체 재시작 후 PTY 복원은 최초 범위에 포함하지 않는다.

기존 모드의 종료 정책은 그대로 유지한다. 새 모드는 정상 종료, 명시적 서비스 중지, 재시작용 detach, 예기치 않은 연결 소실을 구별한다. 아래 ID·제목·PR은 **예정 값**이다. 현재 검증은 CodeSpace `python3 scripts/validate-upstream.py all`과 해당 플랫폼별 stage이며, `python3 scripts/qualify-devguard.py <suite>`는 각 작업에서 제공할 **미구현 예정 명령**이다.

## 재기준, 2026-10-14

이 계획은 실제로 만들지 않은 CS-RG의 C 단위와 예정 PR을 전제로 썼습니다. CS-RG는 소유자가 지시한 U1–U6 단위로 구현되었고, 정확히 한 조합(CodeSpace `ca306ee`, DevGuard 원장 `970acf5`)에 대해 macOS qualification을 마쳤습니다. 그래서 P1-RECOVERY를 시작하면서 진입 조건, 전달 방식, 일부 선행 조건을 실제에 맞춰 다시 잡았습니다. 아래 각 작업의 문제, 불변 조건, 시험, rollback은 그대로 적용하며, 이 표가 그 아래 본문과 다르면 표를 따릅니다.

| 계획 문구 | 실제 | 재기준 |
| --- | --- | --- |
| 진입 CSRG-C08, P1R-P1의 선행 PR CSRG-P5 | CSRG-C08은 만들지 않았습니다 | 진입 조건은 CS-RG `implemented`, `macos-qualified`(조합 CodeSpace `ca306ee`) |
| 예정 PR P1R-P1–P1R-P3, 각각 작업 두 개 | CS-RG는 소유자가 지시한 단위마다 PR 하나로 진행했습니다 | P1R-C01부터 P1R-C06까지 단위마다 CodeSpace PR 하나를 차례로 병합. 아래 예정 PR 표는 이력 |
| P1R-C01: 독립 시작과 서비스 소유 | gateway가 관리 worker를 띄우거나, `--runner-socket`으로 이미 있는 worker에 붙습니다 | 독립 Runner는 운영자나 qualification 하네스가 띄우고 gateway는 연결만 합니다. 독립 Runner는 소비자 자격 파일을 직접 읽습니다([ADR-001](../decisions.md#adr-001--실행-소유자인-runner의-단일-등록)) |
| P1R-C02: process 전용 기록과 single writer | operations DB에는 patch operations와 승인이 있고, `--operations-db`가 없으면 DB는 메모리에 있습니다 | process 기록은 operations DB 안의 별도 표이며 schema 버전을 가집니다. 독립 모드에서는 `--operations-db`가 필수이고, 한 번에 gateway 하나가 배타 lock을 잡고 씁니다. 이전 gateway가 끝나기를 상한 안에서 기다립니다 |
| P1R-C03: control/data session의 원자적 결합 | Runner wire는 한 stream이고 두 번째 lane은 없습니다 | 한 연결을 유지합니다. 제어권은 다음 epoch를 발급하는 `Claim`으로 넘기고, 상태를 바꾸는 요청은 모두 그 epoch를 가집니다 |
| P1R-C03: 보호된 endpoint와 자격, 잘못된 자격 거절 | 지금 worker는 연결을 한 번만 받습니다. 같은 UID 확인만으로는 그 사용자의 어떤 프로세스든 받아들입니다 | 살아 있는 controller가 있으면 `Claim`을 거부하고, peer UID, operations store id, epoch를 확인합니다. gateway가 없는 사이에는 같은 사용자의 어떤 프로세스든 `Claim`할 수 있으며, 운영자 안내에 이를 적습니다. 잘못된 자격 시험은 다른 UID, controller가 있을 때의 `Claim`, 다른 store id로 바뀝니다 |
| P1R-C04: CS-RG attempt·approval mapping | 승인과 attempt나 프로세스를 잇는 기록이 없습니다 | P1R-C02의 기록이 승인, attempt, 프로세스를 처음으로 잇습니다 |
| P1R-C05: owner 손실 뒤 미해결 lease | `Lookup`은 attempt를 소유한 instance만 할 수 있고, CodeSpace의 `codespace` 소비자는 instance가 하나입니다 | Runner 손실 뒤 운영자 runbook이 관리 요청 `Quiescence`로 남은 attempt를 확인하고, 이전 instance가 retire된 뒤에 새 Runner가 등록합니다. `off` 점유는 운영자가 프로세스별로 확인해 풉니다 |
| 작업마다 `qualify-devguard.py recovery-*` 명령 | CS-RG는 마지막 단위에서 하네스를 더했습니다 | P1R-C01–P1R-C05는 CodeSpace CI의 통합 시험으로 확인하고, P1R-C06이 하네스 suite를 모두 더합니다 |
| P1R-C06: 통과한 CS-RG 기준선 | CS-RG 기준선은 후보 `ca306ee`입니다 | P1R-C06은 P1-RECOVERY 후보에서 CS-RG suite도 다시 돌립니다. 범위는 독립 모드의 macOS `uds`, pipe와 PTY, `off`와 `required`이고, Linux `off`는 CI에서만 확인합니다 |

이 마일스톤을 위해 release, DevGuard client pin, Codex pin, 설치된 서비스, 정책을 소유자 승인 없이 바꾸지 않습니다. 뒤 단위가 기대는 선택은 그 단위를 시작할 때 소유자에게 확인합니다.

## PR 순서와 활성화

| 예정 PR | 작업 | 선행 PR | 함께 검토할 경계 |
| --- | --- | --- | --- |
| P1R-P1 | P1R-C01, P1R-C02 | CSRG-P5 | opt-in 수명과 process 전용 기록; 복구 활성화 전 상태 일관성 |
| P1R-P2 | P1R-C03, P1R-C04 | P1R-P1 | 제어권 fence와 workspace/승인/lease 대조를 함께 제공 |
| P1R-P3 | P1R-C05, P1R-C06 | P1R-P2 | 관측 손실·Runner 손실 처리와 반복 장애 qualification |

### P1R-C01 — 독립 Runner 모드·수명·capability

- 소유/예정 PR: CodeSpace / P1R-P1. 예정 제목: `feat(recovery): add an explicit independent Runner lifecycle`.
- 문제 → 동작: 현재 Gateway-owned worker의 kill-on-drop 정책을 유지하면서 별도 운영자 선택 모드만 Gateway 단절 뒤 생존시킨다.
- 선행: CSRG-C08. 독립 service manager/운영자 시작과 endpoint 권한, 살아 있는 Runner의 timeout 담당자.
- 대상/산출물: server config/runtime, Runner startup/hello, lifecycle intent와 capability, mode별 종료 표.
- 불변 조건: default/기존 UDS/InProcess 정책 보존; 같은 PID 복구 주장 금지; Gateway detach가 새 작업 admission 아님.
- 시험: 정상 detach와 재시작; explicit stop·잘못된 mode 요청 거절; 정상 종료와 연결 소실 동시 발생, 원래 timeout 중 Gateway crash.
- 검증 명령: 현재 upstream runtime/disconnect 회귀 + 예정 `python3 scripts/qualify-devguard.py recovery-lifecycle`.
- 완료 증거: mode/intent별 process·Runner 생존 및 종료 결과, capability gating, 원래 deadline 유지.
- rollback: 새 mode 신규 시작을 닫고 기존 Runner를 drain; 살아 있는 작업을 이전 Gateway-owned mode로 무단 전환하지 않는다.
- 인계: P1R-C02에 process 소유자·epoch·deadline 기록 대상. P1만으로 재연결 qualification을 선언하지 않는다.

### P1R-C02 — process 전용 영속 기록

- 소유/예정 PR: CodeSpace / P1R-P1. 예정 제목: `feat(recovery): persist process identity separately from patch operations`.
- 문제 → 동작: Gateway 메모리 손실 후 동일 실행을 식별하되 PID/DB가 handle을 복원한다는 잘못된 의미를 부여하지 않는다.
- 선행: P1R-C01. 살아 있는 Runner가 I/O owner이며 process 기록의 single writer/트랜잭션 소유자가 정해져야 한다.
- 대상/산출물: 예정 process store/schema와 migration, process_id·Runner epoch·boot/start identity·attempt/lease·deadline·출력 cursor·terminal reason.
- 불변 조건: patch operations 원장 분리; secret/무조건 재실행 가능한 argv 큐로 사용 금지; 기록 존재만으로 실행/종료 성공 확정 금지.
- 시험: 정상 재조회/terminal 기록; torn write·unknown schema·PID 재사용; spawn/상태 저장/종료 경계 crash와 재조회 경쟁.
- 검증 명령: 현재 upstream store 회귀 + 예정 `python3 scripts/qualify-devguard.py recovery-store`.
- 완료 증거: schema fixture, crash point별 durable/unknown 결과, patch 원장 불변과 민감값 미저장.
- rollback: 신규 mode 정지와 호환 reader 보존; 새 기록이 있는 DB를 과거 snapshot으로 되돌리지 않는다.
- 인계: P1R-C03에 식별/epoch 대조 API, P1R-C04에 승인·workspace 연계 키를 전달한다.

### P1R-C03 — 인증 재연결과 제어권 fence

- 소유/예정 PR: CodeSpace / P1R-P2. 예정 제목: `feat(recovery): fence stale Gateways during authenticated reconnect`.
- 문제 → 동작: 동일 Runner에 두 Gateway가 동시에 mutation을 보내지 못하게 인증된 새 연결에 epoch/제어권을 부여한다.
- 선행: P1R-C02. 보호된 endpoint·자격, durable owner/epoch 갱신, control/data session 원자적 결합.
- 대상/산출물: Runner handshake·mutation fence, Gateway reconnect/backoff와 observer 구분, stale-controller 오류.
- 불변 조건: 오래된 epoch의 exec/stdin/resize/terminate mutation 거절; 조회 허용 범위 명시; 재연결에 새 workload budget 발급 불필요.
- 시험: 정상 새 Gateway 인계; 잘못된 자격·재사용 session 거절; 동시 두 Gateway·지연 mutation·half-open lane·epoch 갱신 응답 유실.
- 검증 명령: 현재 upstream wire/auth 회귀 + 예정 `python3 scripts/qualify-devguard.py recovery-fence`.
- 완료 증거: epoch별 유일 mutation 권한, 원래 process_id/attempt 유지, 재연결 중 중복 실행·예약 0.
- rollback: reconnect 신규 권한 발급을 닫고 마지막 유효 제어권으로 stop/drain; epoch를 감소시키지 않는다.
- 인계: P1R-C04와 같은 PR에서 대조 완료 전 mutation을 여는 경로를 차단한다.

### P1R-C04 — workspace·승인·lease 대조

- 소유/예정 PR: CodeSpace / P1R-P2. 예정 제목: `feat(recovery): reconcile workspace approvals and resource leases`.
- 문제 → 동작: Gateway가 잃은 busy 상태나 오래된 approval로 동일 workspace에서 실행이 겹치지 않도록 Runner의 실제 상태와 대조한다.
- 선행: P1R-C03. P1R-C02 기록, CSRG attempt/approval mapping, 기존 실행 조회 가능; DevGuard 신규 admission은 요구하지 않는다.
- 대상/산출물: store workspace occupancy 복원, approval resume reconciliation, client lease 관측, 복구 완료 barrier.
- 불변 조건: 살아 있는 실행의 workspace 점유 유지; consumed/unknown을 queued로 추정 복귀 금지; DevGuard 장애에도 기존 handle 조회/종료 제공.
- 시험: 정상 running/terminal 재연결; stale approval·authority unavailable·기록 불일치; reconnect 중 종료/event와 새 workspace 요청 경쟁.
- 검증 명령: 현재 upstream workspace/approval 시험 + 예정 `python3 scripts/qualify-devguard.py recovery-reconcile`.
- 완료 증거: 각 실행의 workspace/approval/lease 대응표, 불일치 격리 및 mutation barrier, 이중 실행 0.
- rollback: 신규 mutation 정지, 기존 process를 관측·종료·대조; busy를 일괄 해제하지 않는다.
- 인계: P1R-C05에 복구한 출력 cursor와 terminal/unknown 상태, 미해결 lease 목록을 넘긴다.

### P1R-C05 — 출력·종료 관측과 Runner 손실

- 소유/예정 PR: CodeSpace / P1R-P3. 예정 제목: `feat(recovery): preserve uncertainty and output gaps after owner loss`.
- 문제 → 동작: 재연결한 Gateway가 출력 누락과 실제 종료를 구분하고 Runner/호스트 소실을 성공적인 process 종료로 보고하지 않는다.
- 선행: P1R-C04. 살아 있는 Runner의 bounded output buffer/cursor와 actual termination 관측, host boot 확인.
- 대상/산출물: read/status/terminal reconciliation, output gap/retention metadata, Runner-loss runbook과 관측 fixture.
- 불변 조건: PID만으로 pipe/PTY 재생성 금지; 원래 timeout 연장 금지; argv 자동 실행 금지; 자원 lease 반환이 승인 재사용 증거는 아님.
- 시험: 정상 buffered output·종료 재조회; buffer TTL 초과·Runner crash·host reboot; 종료와 reconnect·buffer eviction 경쟁, PID 재사용.
- 검증 명령: 현재 upstream process/output 시험 + 예정 `python3 scripts/qualify-devguard.py recovery-observation`.
- 완료 증거: gap/unknown/terminal 구분, scope 증거 없을 때 회계 유지, 원래 deadline과 자동 실행 수 0.
- rollback: 새 모드의 신규 실행을 닫고 지원 관측 경로 유지; 미해결 기록을 성공으로 일괄 종결하지 않는다.
- 인계: P1R-C06에 반복 재시작 및 손실 matrix, 알려진 출력 보존 범위를 제공한다.

### P1R-C06 — 반복 Gateway 재시작 qualification

- 소유/예정 PR: CodeSpace / P1R-P3. 예정 제목: `test(recovery): qualify repeated Gateway restarts with a live Runner`.
- 문제 → 동작: 단일 reconnect 데모가 아닌 부하·승인·출력·제어권 경쟁에서 복구 범위와 한계를 입증한다.
- 선행: P1R-C05. 고정 source/artifact/정책, 독립 Runner, 재시작 횟수·시점 fixture, 검증된 CSRG baseline.
- 대상/산출물: recovery qualification harness, 기존 모드 regression, 반복 trace 및 운영자 도입/복귀 문서.
- 불변 조건: 새 capability의 범위는 Gateway 생존 복구로 한정; 기존 모드 disconnect 보존; source/timeout/attempt 일관성.
- 시험: pipe/PTY 장기 실행 중 반복 Gateway 재시작; malformed state·Runner 손실·authority 장애; 동시 Gateway·승인/종료·출력 포화 경쟁, 관제/foreground SLO.
- 검증 명령: 현재 upstream 전체·platform별 gate + 예정 `python3 scripts/qualify-devguard.py recovery`; idle10분/부하30분 이상/3회와 원시값 보존.
- 완료 증거: 동일 실행 재연결·중복 0·stale mutation 0·timeout 연장 0·명확한 gap, 기존 모드 regression 통과와 지원 mode manifest.
- rollback: 새 mode rollout 중지 후 독립 Runner drain, 이전 검증 조합으로 복귀; Runner 자체 재시작 복원을 통과한 것처럼 기록하지 않는다.
- 인계: 기존 CodeSpace 다음 우선 경로로 넘긴다. Runner/I/O owner 자체 복구 확대는 새로운 설계와 승인을 거친다.
