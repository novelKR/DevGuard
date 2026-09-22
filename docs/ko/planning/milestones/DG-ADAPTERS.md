# DG-ADAPTERS — 추가 도구와 실행 환경의 소비

소유: DevGuard. 상태: `not-started` / `not-run`. 진입: DG1-C12. P1-RECOVERY의 선행 조건이 아니다. 완료: 각 지원 도구/version/실행 위치에서 argv·옵션·FD 의미를 보존하고 실제 자식·executor 수명까지 검증한 지원표.

아래 ID·제목·PR은 **예정 값**이다. 도구 flags는 구현 시 해당 pinned 도구의 공식 계약과 대조하고 지원하지 않는 조합을 거절한다. 언어별 예외는 adapter에 두고 generic authority에 누적하지 않는다. 현재 `python3 scripts/validate.py --offline`은 DG-0 회귀다. 아래 `python3 scripts/qualify.py <suite>`는 **미제공 예정 명령**이며 기능과 실제 fixture를 같은 PR에 제공한다.

| 예정 PR | 작업 | 선행 PR/추가 조건 | 활성화 경계 |
| --- | --- | --- | --- |
| DGA-P1 | DGA-C01, DGA-C02 | DG1-P6 | Python 변환과 충돌/중첩 시험 |
| DGA-P2 | DGA-C03, DGA-C04 | DG1-P6 | JS worker/heap 변환과 자식 검증 |
| DGA-P3 | DGA-C05, DGA-C06 | DG1-P6 | jobserver 연계와 FD/중첩 예산 검증 |
| DGA-P4 | DGA-C07, DGA-C08 | DGA-P1~P3, Linux 강제 보호는 DGL-C06 | 실제 executor binding과 수명 검증 |

P1~P3은 같은 선행 기반 위에서 독립적으로 준비할 수 있다. 최초 제출 순서는 표 순서를 권장한다. P4는 지원하려는 adapter 조합의 P1~P3 결과와 실제 executor 자격을 모아 검증한다. DGL 추가 조건은 작업 단위의 조건이며 milestone ledger의 기본 의존 관계를 덮어쓰지 않는다.

### DGA-C01 — Python·pytest 변환

- 소유/예정 PR: DevGuard / DGA-P1. 예정 제목: `feat(adapters): translate supported Python and pytest workloads`.
- 문제 → 동작: generic 명령에 임의 환경변수를 주입하는 대신 명시 Python/pytest adapter가 확인된 병렬 옵션만 조정한다.
- 선행: DG1-C12. 도구/version·plugin inventory, 활성 interpreter/환경 경로와 부모 lease, 명시 adapter 선택.
- 대상/산출물: 예정 adapter registry의 Python module, argv/env diff receipt, supported/unsupported options 표와 fixture.
- 불변 조건: interpreter·venv·cwd·test selection·exit 의미 보존; plugin 없는 worker 제한을 있다고 표시 금지; 환경 디렉터리 GC 금지.
- 시험: 정상 interpreter/pytest 선택; plugin 없음·버전 불일치·명시 옵션 충돌; 두 소비자 병렬과 parent cancel 중 worker 시작.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py python-transform`.
- 완료 증거: 변환 전후 argv·test 집합·exit 일치, 적용 가능한 병렬도 근거와 자식 budget 대응.
- rollback: adapter 지원을 닫고 검증된 generic 소비로 명시 전환; 사용자 환경/옵션을 조용히 고치지 않는다.
- 인계: DGA-C02와 함께 오류·중첩 fixture를 통과할 때만 지원표에 등재한다.

### DGA-C02 — 옵션·중첩·미지원 시험

- 소유/예정 PR: DevGuard / DGA-P1. 예정 제목: `test(adapters): verify Python nesting and unsupported combinations`.
- 문제 → 동작: 일치하는 간단한 옵션만 시험해 실제 nested runner가 별도 병렬 풀을 만드는 누락을 막는다.
- 선행: DGA-C01. pinned Python/pytest/plugin fixture, nested invocation과 signal 관측.
- 대상/산출물: adapter 변환 회귀·실제 worker 수·nested parent budget tests, unsupported 진단·문서.
- 불변 조건: 누락된 제한은 unsupported/accounting으로 정직하게 표시; 사용자 explicit flag의 우선/거절 계약 보존.
- 시험: 정상 연속·중첩 실행; 잘못된 flag·plugin/환경 없음; child spawn과 취소·동시 nested 실행에서 이중 budget 0.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py python-adapter`.
- 완료 증거: 선택 테스트·종료값 동일, worker peak와 lease 합계, 환경 hash/경로 보호 및 지원 version matrix.
- rollback: 실패 조합을 지원표에서 제거·신규 실행 거절; 현재 child를 관측 후 정리한다.
- 인계: 검증된 Python 조합을 DGA-C07의 환경 binding 후보로 전달한다.

### DGA-C03 — Node·Jest·Bun 변환

- 소유/예정 PR: DevGuard / DGA-P2. 예정 제목: `feat(adapters): translate supported Node Jest and Bun controls`.
- 문제 → 동작: 서로 다른 runtime/test runner의 worker·heap 옵션을 실제 지원 version별로 변환한다.
- 선행: DG1-C12. pinned runtime/runner·package lock·parent lease, 적용할 명시 옵션과 parser contract.
- 대상/산출물: 예정 JS adapter modules, worker/heap 계획·argv/env receipt, version별 capability와 fixture.
- 불변 조건: heap 제한은 전체 프로세스/RSS 제한 아님; Node flag를 Bun에 무조건 전달하지 않음; scripts·test selection·lockfile 보존.
- 시험: 정상 Node/Jest/Bun 각각의 지원 실행; unknown version/충돌 flag·미지원 기능; 동시 runner와 script가 추가 child를 시작하는 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py javascript-transform`.
- 완료 증거: 각 도구의 적용 옵션과 확인 방법, 동일 script 결과, worker·heap 요구와 실제 관측 분리.
- rollback: 해당 도구 지원을 비활성화·명시 generic 경로 선택; package/lockfile 자동 수정 금지.
- 인계: DGA-C04에서 자식과 전체 memory 관측까지 검증하며 두 commit을 함께 제출한다.

### DGA-C04 — heap·worker·자식 실행 검증

- 소유/예정 PR: DevGuard / DGA-P2. 예정 제목: `test(adapters): distinguish heap settings from process scope limits`.
- 문제 → 동작: heap flag 성공을 자식 포함 전체 memory 강제 보호로 잘못 보고하는 것을 차단한다.
- 선행: DGA-C03. child process fixture·heap/RSS 구분 관측, 실제 OS scope 수준과 도구별 runner.
- 대상/산출물: JS integration suite·peak memory/worker counts·supported limits 설명.
- 불변 조건: requested/supported/applied 자원별 수준 보존; spawned child가 독립 전체 budget을 얻지 않음; unsupported hard requirement 거절.
- 시험: 정상 worker 수와 heap 옵션; native allocation·child memory·flag 무시 버전; 출력 포화/취소/child 종료 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py javascript-adapter`.
- 완료 증거: runtime별 heap와 전체 scope 관측값, 자식 종료·lease 회수, 시험하지 않은 runtime/version 명시.
- rollback: 실패한 제한 claim과 지원 조합 철회, live scope 관측 후 정리; hard→advisory 자동 강등 금지.
- 인계: 검증된 JS 조합과 실제 한계를 DGA-C07에 넘긴다.

### DGA-C05 — make·ninja·jobserver 연계

- 소유/예정 PR: DevGuard / DGA-P3. 예정 제목: `feat(adapters): coordinate build tools through inherited jobservers`.
- 문제 → 동작: Cargo/make/ninja가 각각 병렬도 풀을 새로 발급해 상위 budget을 중복 소비하지 않도록 토큰의 소유권을 연결한다.
- 선행: DG1-C12. DG1-C08 jobserver 계약, 실제 tool/version별 지원 여부, inherited FD와 parent lease.
- 대상/산출물: 예정 make/ninja adapter·jobserver bridge, token ownership/FD lifetime와 explicit jobs 정책.
- 불변 조건: 명시 options/MAKEFLAGS 및 필요한 FD 보존; 지원 없는 tool의 jobserver를 있다고 보고 금지; 중첩 풀 합산 상한 유지.
- 시험: 정상 각 build tool; invalid/closed FD·conflicting jobs·미지원 version; nested Cargo/make와 token wait/cancel 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py build-transform`.
- 완료 증거: 실제 전달 FD와 토큰 수, 변환 argv/env·build target 동일, 상위 budget 초과 없는 worker 관측.
- rollback: 새 bridge 진입 중지·빌린 token 반환·자식 종료 대조; 부모 jobserver를 닫거나 교체하지 않는다.
- 인계: DGA-C06에서 중첩·장애 cleanup까지 같은 PR로 검증한다.

### DGA-C06 — 중첩 예산과 FD·옵션 보존

- 소유/예정 PR: DevGuard / DGA-P3. 예정 제목: `test(adapters): preserve nested budgets file descriptors and build options`.
- 문제 → 동작: 정상 빌드뿐 아니라 실패·취소 뒤에도 token 누수와 과잉 병렬도가 없는지 검증한다.
- 선행: DGA-C05. 실제 nested tool graph와 FD inspection fixture, parent budget·child lifecycle 관측.
- 대상/산출물: nested build suite, token accounting/fault injector·옵션 보존 표·지원 버전 목록.
- 불변 조건: 자격 FD는 payload 전 닫고 jobserver FD는 필요한 자식에만 유지; 독립 host budget 추가 발행 금지.
- 시험: 정상 중첩 빌드 산출물·exit; child crash·닫힌 FD·signal; 토큰 대기/반환/부모 취소 경쟁과 반복 실행.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py build-adapter`.
- 완료 증거: 전후 token 수·FD 목록·최대 동시 worker·parent/child lease tree, 반복 후 누수 0.
- rollback: 지원표에서 실패 graph 제외, parent 살아 있는 동안 정리·토큰 반환; 명령을 자동 재실행하지 않는다.
- 인계: DGA-C07에서 사용할 검증된 nested graph와 budget/FD 계약을 전달한다.

### DGA-C07 — VM·container 실행 위치와 예산

- 소유/예정 PR: DevGuard / DGA-P4. 예정 제목: `feat(adapters): bind budgets to actual VM and container executors`.
- 문제 → 동작: 로컬 Docker CLI의 위치와 실제 실행 host/guest를 구분하고 물리 용량을 중복 계산하지 않는다.
- 선행: DGA-C02, DGA-C04, DGA-C06. Linux 강제 제어 조합은 DGL-C06 추가 필수; executor API/권한·안정 identity·guest authority/상위 scope 관계 확인.
- 대상/산출물: 예정 executor binding adapter, host/guest capacity provenance·control mapping·지원 remote topology 표.
- 불변 조건: VM/컨테이너를 별도 full-host budget으로 합산 금지; 실제 실행 host authority 사용; 원격 daemon capability를 로컬 관측으로 대신하지 않음.
- 시험: 정상 local/remote executor 식별; permission/identity/capability 누락; endpoint 변경·동시 VM 시작·parent lease 취소 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py executor-binding`.
- 완료 증거: CLI→executor→host/guest→scope 대응과 용량 원천, 자원별 강제 수준·지원하지 않는 topology 거절.
- rollback: 신규 executor launch를 닫고 실제 API로 stop/대조; CLI kill만으로 회계 반환하지 않는다.
- 인계: DGA-C08에 실행 위치와 실제 수명 소유자를 전달하며 binding만으로 운영 자격을 부여하지 않는다.

### DGA-C08 — 실제 executor 수명·제약 qualification

- 소유/예정 PR: DevGuard / DGA-P4. 예정 제목: `test(adapters): qualify executor lifetime and enforced resource scopes`.
- 문제 → 동작: Docker CLI 종료와 container 종료를 구분하고 VM/remote 연결 소실 뒤 실제 workload 상태를 대조한다.
- 선행: DGA-C07. 실제 지원 executor·Linux qualification(해당 시)·관측 API·독립 stop 경로·고정 fixture.
- 대상/산출물: executor fault suite·지원 topology manifest·scope exit/unknown 증거·기존 adapter 회귀.
- 불변 조건: 연결 소실·CLI exit만으로 lease 반환 금지; 실제 종료 증거, host/guest 중복 회계 0; payload 자동 재실행 금지.
- 시험: 정상 실행/종료/상한; remote 단절·VM pause·CLI crash·executor 권한 상실; 동시에 cancel/reconnect/stop과 자손 종료 경쟁, 3회 부하 SLO.
- 검증 명령: 현재 DG 및 해당 CodeSpace 기존 gate + 예정 `python3 scripts/qualify.py executors`.
- 완료 증거: 실제 executor identity·종료 readback·원시 scope/latency 값, 지원 topology별 10분 baseline/30분 이상 부하×3, 미실행 조건 분리.
- rollback: 검증 실패 topology 신규 사용 금지·실제 executor 관측/stop 경로 유지; 강제 제어 없는 환경으로 조용히 전환하지 않는다.
- 인계: 검증한 tool/version/executor 조합만 지원표에 반영. 새로운 언어·제품의 실제 코드는 후속 소비 검토에서 별도로 확인한다.
