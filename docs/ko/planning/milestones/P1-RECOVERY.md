# P1-RECOVERY — 독립 Runner 생존 중 Gateway 복구

소유 저장소: CodeSpace. 상태: `not-started` / `not-run`. 진입: CSRG-C08. 완료: 운영자 선택 모드에서 독립 Runner가 기존 프로세스·입출력·timeout을 계속 소유하고 Gateway 반복 재시작과 재연결을 복구한다. InProcess 및 Runner 자체 재시작 후 PTY 복원은 최초 범위에 포함하지 않는다.

기존 모드의 종료 정책은 그대로 유지한다. 새 모드는 정상 종료, 명시적 서비스 중지, 재시작용 detach, 예기치 않은 연결 소실을 구별한다. 아래 ID·제목·PR은 **예정 값**이다. 현재 검증은 CodeSpace `python3 scripts/validate-upstream.py all`과 해당 플랫폼별 stage이며, `python3 scripts/qualify-devguard.py <suite>`는 각 작업에서 제공할 **미구현 예정 명령**이다.

| 예정 PR | 작업 | 선행 PR | 함께 검토할 경계 |
| --- | --- | --- | --- |
| P1R-P1 | P1R-C01, P1R-C02 | CSRG-P4 | opt-in 수명과 process 전용 기록; 복구 활성화 전 상태 일관성 |
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
