# DG-LINUX — 실제 Linux 강제 보호

소유: DevGuard + CodeSpace. 상태: `not-started` / `not-run`. 마일스톤 진입: CSRG-C08. 제품 전체 완료의 필수 조건이며 macOS P1-RECOVERY보다 먼저 끝나야 한다는 추가 의존은 만들지 않는다. 완료: 실제 Linux controller·ancestor·권한 조건에서 자원별 적용, 전체 실행 scope와 종료·회수를 검증한 소비 조합.

아래 ID·제목·PR은 **예정 값**이다. DGL-P1~P3은 논리적인 3개 묶음이며 실제 runtime commit의 주 저장소는 DevGuard다. CodeSpace 실행 순서·adapter 변경이 필요하면 그 묶음의 연계 PR로 별도 제출하고 양쪽 head를 함께 검증한다. 46개 작업/23개 묶음 수는 보장되지 않은 미래 GitHub PR 번호나 개수를 의미하지 않는다. 현재 `Backend`의 fake cgroup 시험은 실제 kernel 검증이 아니다.

| 예정 PR | 작업 | 선행 PR | 안전 경계 |
| --- | --- | --- | --- |
| DGL-P1 | DGL-C01, DGL-C02 | CSRG-P5 | 유효 용량 probe와 실제 자원 제어 |
| DGL-P2 | DGL-C03, DGL-C04 | DGL-P1 | payload 전 containment와 자손 종료·정리 |
| DGL-P3 | DGL-C05, DGL-C06 | DGL-P2 | 실제 환경 fault/pressure와 제품 qualification |

현재 명령: DevGuard `python3 scripts/validate.py --offline`; CodeSpace `python3 scripts/validate-upstream.py linux-isolation` 및 기존 `all` gate. 전자는 fake 계약이며 후자는 기존 sandbox 시험이다. 미래 DevGuard Linux 기능을 대신 검증하지 않는다. `python3 scripts/qualify.py <suite>`는 **미제공 예정 명령**이다. 각 실제 환경의 cgroup delegation·권한과 ancestor 설정을 기록하고 지원되지 않는 CI runner에서는 `not_run`/`inconclusive`로 남긴다.

### DGL-C01 — controller·권한·ancestor 용량

- 소유/예정 PR: DevGuard / DGL-P1. 예정 제목: `feat(linux): probe delegated controllers and effective capacity`.
- 문제 → 동작: 호스트 총량만 보고 부모의 더 작은 상한을 넘게 할당하는 문제를 막고 실제 사용 가능한 용량을 산정한다.
- 선행: CSRG-C08. 실제 cgroup v2·위임된 하위 경로·읽기/쓰기 권한, 신뢰할 실행 위치·boot identity.
- 대상/산출물: 예정 `crates/platform-linux` probe, controller/ancestor/cpuset·memory·pids 관측 receipt와 거절 이유. 새 crate의 의존 검증 확장 포함.
- 불변 조건: requested/supported/applied 분리; ancestor 제한보다 큰 budget 발행 금지; probe 실패를 무제한으로 해석하지 않는다.
- 시험: 정상 위임 계층; controller 누락·read-only·권한 거절; ancestor 변경/컨테이너 이동과 admission 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py linux-probes`.
- 완료 증거: 실제 mount/controller/ancestor readback과 effective capacity 계산; 지원 불가를 구분한 matrix.
- rollback: Linux admission 닫기, 기존 scope 보존·관측; 상위 cgroup 설정을 자동 확대하지 않는다.
- 인계: DGL-C02에 검증된 유효 예산과 제어 가능한 root만 전달한다.

### DGL-C02 — 제어·작업 cgroup과 상한

- 소유/예정 PR: DevGuard / DGL-P1. 예정 제목: `feat(linux): separate control and workload resource scopes`.
- 문제 → 동작: 작업 포화가 관리 서비스의 예약을 소비하지 않도록 제어와 workload scope를 분리하고 자원별 상한을 적용한다.
- 선행: DGL-C01. ancestor 예산 내 정적 제어 예약, controller 위임과 scope 생성 권한.
- 대상/산출물: 예정 Linux apply/scope backend, aggregate·lease별 CPU/memory/pids 계획과 readback, capability 표.
- 불변 조건: `cpu.max`를 전용 코어로 설명하지 않음; `memory.min`을 물리 메모리 선할당으로 설명하지 않음; 실제 적용 확인 전 kernel level 주장 금지.
- 시험: 정상 상한 적용·거절; 일부 파일 write 실패·ancestor 부족; 동시 lease 생성과 정책 변경·scope 정리 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py linux-controls`.
- 완료 증거: 자원별 요구/적용/효과 측정과 실패 시 실행 허용 차단, 제어 서비스 scope의 별도 관측.
- rollback: 미실행 빈 scope 정리; live cgroup 상한 제거로 rollback하지 않고 drain 후 호환 정책 적용.
- 인계: DGL-C03에 payload를 허용하기 전 완성해야 하는 containment API를 전달한다.

### DGL-C03 — sandbox·proxy를 포함한 실행 순서

- 소유/예정 PR: DevGuard 주 소유, CodeSpace 연계 / DGL-P2. 예정 제목: `feat(linux): contain launch helpers sandbox and proxies before exec`.
- 문제 → 동작: user payload만 scope에 넣고 sandbox/helper/proxy 비용과 자손이 빠지는 경우를 방지한다.
- 선행: DGL-C02. DG1 launch fence, CodeSpace `process.rs`·linux-sandbox·network proxy의 실제 spawn 경로 검토와 지원 실행 조합.
- 대상/산출물: Linux launcher bind 순서, CodeSpace adapter hook(필요 시 연계 PR), scope 멤버십 trace와 credential FD 경로.
- 불변 조건: 사용자 executable 전에 모든 관리 대상 scope 준비; private credential payload 누출 금지; CodeSpace sandbox 정책을 DevGuard로 이전하지 않음.
- 시험: 정상 pipe/PTY·proxy 실행; attach/bind 실패·sandbox 오류; helper 지연·cancel·자손 fork 경쟁에서 범위 밖 실행 0.
- 검증 명령: 현재 CodeSpace `linux-isolation integration` + 예정 `python3 scripts/qualify.py linux-launch`.
- 완료 증거: helper→sandbox→proxy→payload 단계의 실제 membership·READY/exec 구분, 연계 head/artifact 조합.
- rollback: 신규 launch 중지 후 DGL-C04 경로로 모든 scope 대조; launcher hook만 되돌려 live scope를 잃지 않는다.
- 인계: DGL-C04와 같은 논리 PR 묶음에서 종료·실패 경로까지 제공한다.

### DGL-C04 — 자손 종료·회수·OOM·권한 실패

- 소유/예정 PR: DevGuard / DGL-P2. 예정 제목: `feat(linux): reconcile descendant termination and resource failures`.
- 문제 → 동작: root exit/OOM notification을 전체 scope 소멸과 혼동하지 않고 실제 자손 종료 뒤 회계 회수를 수행한다.
- 선행: DGL-C03. exact scope identity·actual membership 관측·종료 권한, 이벤트/관측 재시도 경로.
- 대상/산출물: Linux reconciler·OOM/termination reason·권한 장애 처리·shutdown fixture.
- 불변 조건: scope empty와 root reap 등 필요한 증거 충족; tracking loss sticky; OOM을 성공 exit 또는 안전한 재시작으로 분류하지 않음.
- 시험: 정상 전체 종료; root만 종료·OOM·signal 거절·scope 읽기 실패; 재시작/종료/자손 생성 경쟁, PID reuse.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py linux-reconcile`.
- 완료 증거: 생존 자손 동안 charge 유지, 실제 회수 이유·종료 시간·권한 오류; 중복 재할당 0.
- rollback: 신규 admission 차단, 불확실 scope charge 보존·운영자 대조. cgroup 디렉터리 제거를 종료 증거로 삼지 않는다.
- 인계: DGL-C05에 장애 주입 지점과 회수/unknown 기대값을 전달한다.

### DGL-C05 — 실제 Linux 압력·장애 시험

- 소유/예정 PR: DevGuard / DGL-P3. 예정 제목: `test(linux): exercise real controllers under pressure and faults`.
- 문제 → 동작: fake backend 시험과 구분되는 kernel·권한·ancestor 증거를 확보한다.
- 선행: DGL-C04. 제한된 실제 Linux 시험 호스트, 제어 여유, 고정 fixture와 재부팅/위임 실패 범위.
- 대상/산출물: Linux qualification harness·제어/작업 계측·fault report, runner capability 탐지.
- 불변 조건: unavailable controller 시험을 pass로 치환 금지; host 관리 범위를 넘어 ancestor 임의 변경 금지; raw evidence 보존.
- 시험: 정상 CPU/memory/pids 압력; OOM·위임 철회·daemon crash; 동시 소비/취소/재시작과 controller 변경 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py linux-faults`.
- 완료 증거: kernel/OS/ancestor/권한/clock·source/artifact와 자원별 강제 효과, 관제 생존 및 회계 일관성.
- rollback: fixture 중단·관리 scope drain·시험 설정 복원 확인; 보존 증거를 cache sweep에 포함하지 않는다.
- 인계: DGL-C06에 실제 통과한 환경·자원 수준·한계만 전달한다.

### DGL-C06 — Linux CodeSpace 조합 qualification

- 소유/예정 PR: DevGuard 주 소유, CodeSpace 검증 연계 / DGL-P3. 예정 제목: `test(qualification): qualify Linux consumer and control protection`.
- 문제 → 동작: kernel 단독 성공을 실제 Runner·MCP·sandbox 조합 성공으로 확대하는 오류를 막는다.
- 선행: DGL-C05와 CSRG-C08. 실제 Linux executor에서 pin/artifact/wire/정책 조합 고정, remote RTT와 local 관제 측정 분리.
- 대상/산출물: 조합 manifest·CodeSpace 기존 Linux CI/upstream 결과·통합 report·지원 환경표.
- 불변 조건: DG-LINUX는 전체 제품 완료 필수; macOS 증거 재사용 불가; 복구 capability는 P1R qualification 여부에 따라 별도 표시.
- 시험: 정상 pipe/PTY·관제·sandbox/proxy; authority/권한/OOM 장애; 포화 중 replay·approval·terminate 경쟁과 3회 SLO.
- 검증 명령: 현재 CodeSpace `all`/실제 Linux isolation + 예정 `python3 scripts/qualify.py linux-consumer` (연계 CodeSpace harness 호출 포함).
- 완료 증거: 두 저장소 exact head, 실제 controller readback·raw latency·부하30분 이상×3/idle10분, 실패·skip 없는 지원 범위.
- rollback: 해당 Linux 조합의 신규 required 소비 중지·scope drain 후 검증 조합 복귀; hard level을 조용히 accounting으로 낮추지 않는다.
- 인계: 제품 전체 완료 평가 및 DGA-C07의 Linux VM/container 조건. kernel 지원이 보장하지 않는 절대 성능은 별도 측정으로 남긴다.
