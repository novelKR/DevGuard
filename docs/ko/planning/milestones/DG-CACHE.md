# DG-CACHE — 등록된 캐시의 안전한 회수

소유: DevGuard. 상태: `not-started` / `not-run`. 진입: DG1-C12. P1-RECOVERY의 선행 조건이 아니다. 완료: 등록 root의 활성 사용과 reclaim이 상호 배제되고 보호 대상 삭제 0건, 실제 가용 공간 변화까지 증명한 조합.

아래 작업·제목·PR은 **예정 값**이다. 예정 `crates/cache`가 추가되면 의존 경계 검증도 함께 확장한다. 현재 명령은 `python3 scripts/validate.py --offline`이며 캐시 기능을 검증하지 않는다. `python3 scripts/qualify.py <suite>`는 각 구현 PR이 제공할 **미제공 예정 명령**이다.

| 예정 PR | 작업 | 선행 PR | 함께 제공할 안전 경계 |
| --- | --- | --- | --- |
| DGC-P1 | DGC-C01, DGC-C02 | DG1-P6 | 등록·보호와 active-use 상호 배제; 아직 삭제 안 함 |
| DGC-P2 | DGC-C03, DGC-C04 | DGC-P1 | mark/rename과 중단·재시작 가능한 sweep |
| DGC-P3 | DGC-C05, DGC-C06 | DGC-P2 | 유지관리 예산·shared pool 회계·실제 회수 qualification |

### DGC-C01 — root 등록·분류·보호

- 소유/예정 PR: DevGuard / DGC-P1. 예정 제목: `feat(cache): register reclaim roots and protected data classes`.
- 문제 → 동작: 파일명이 cache처럼 보인다는 이유로 운영 상태나 검증 증거를 삭제하지 않도록 명시 root와 분류만 허용한다.
- 선행: DG1-C12. 운영자 등록 root·소유권·filesystem identity, 보호 정책과 실제 cache 재생성 가능성 확인.
- 대상/산출물: 예정 root registry/classifier·보호 rules·읽기 전용 후보 목록. Git, journal, evidence, 안정/복구 artifact, `.venv`, `node_modules`와 CodeSpace `target/upstream-reports/local` 보호.
- 불변 조건: 자동 탐색으로 삭제 범위 확대 금지; root 교체·symlink escape 차단; 사용 환경을 단순 재생성 캐시로 분류하지 않음.
- 시험: 정상 등록/후보 열람; 보호 경로 중첩·권한·symlink 거절; root inode 교체와 스캔 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py cache-roots`.
- 완료 증거: root identity·분류 이유·보호 경로 matrix, 삭제 없는 preview와 보호 대상 포함 0.
- rollback: root 등록 비활성화와 후보 목록 폐기; 파일 변경 없음.
- 인계: DGC-C02에 exact root/entry identity와 보호 판정; 삭제 capability는 아직 닫힌다.

### DGC-C02 — active-use lease와 reclaim 상호 배제

- 소유/예정 PR: DevGuard / DGC-P1. 예정 제목: `feat(cache): exclude active users from reclamation`.
- 문제 → 동작: 검사 시점에는 idle이지만 삭제 전에 사용되는 cache를 lease와 reclaim lock으로 보호한다.
- 선행: DGC-C01. cache 진입점이 사용 lease를 실제 획득한다는 증거; 감시되지 않는 사용자는 자동 삭제 허용 대상에서 제외.
- 대상/산출물: 예정 use/reclaim lease API·generation/fence·재시작 대조, adapter 사용 hooks.
- 불변 조건: active/suspect use가 있으면 reclaim 금지; TTL 경과나 client disconnect만으로 안전한 비사용을 추정하지 않음.
- 시험: 정상 사용 완료 뒤 reclaim 예약; 미등록 사용자·lease 오류·재시작 suspect; use acquire와 reclaim mark의 동시 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py cache-leases`.
- 완료 증거: 경쟁의 유일 승자와 loser 결과, active entry 삭제 0, 재시작 fence 보존.
- rollback: reclaim 신규 예약 중지·active lease 대조; lock 파일 강제 삭제로 우회하지 않는다.
- 인계: DGC-C03에 reclaim 권한과 보호 snapshot을 넘기며 rename 직전 identity를 재확인한다.

### DGC-C03 — mark와 같은 filesystem의 rename

- 소유/예정 PR: DevGuard / DGC-P2. 예정 제목: `feat(cache): mark entries and move them atomically to local trash`.
- 문제 → 동작: 스캔과 삭제 사이 경로 교체를 막고 lease 확보 후 같은 filesystem의 trash로 옮긴다.
- 선행: DGC-C02. 유효 reclaim lease·보호 재검사·같은 filesystem trash와 journal durability.
- 대상/산출물: 예정 mark transaction·identity 재검사·rename·trash ledger와 복구 상태.
- 불변 조건: cross-filesystem copy/delete fallback 금지; symlink 따라가기 금지; trash로 이동한 byte를 회수된 용량으로 계산하지 않음.
- 시험: 정상 mark/rename; EXDEV·권한 오류·보호 대상 변경; use 재획득·root 교체·rename 직전 crash 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py cache-mark`.
- 완료 증거: 원래/새 identity·ledger 단계, 부분 실패 복구, 보호 경로 무변경.
- rollback: sweep 전 같은 identity이고 목적지가 비었을 때만 안전 복원; 충돌이면 격리·대조, 덮어쓰기 금지.
- 인계: DGC-C04와 같은 PR에서 중단 가능한 sweep을 완성한 뒤 삭제 capability를 검토한다.

### DGC-C04 — 중단 가능한 sweep과 재시작 회계

- 소유/예정 PR: DevGuard / DGC-P2. 예정 제목: `feat(cache): resume bounded trash sweeps with durable accounting`.
- 문제 → 동작: sweep 중 crash/압력 변화가 보호 경로 재탐색이나 용량 과대 보고로 이어지지 않게 한다.
- 선행: DGC-C03. 완료된 rename ledger·정확한 trash root·유한 작업 batch와 취소 신호.
- 대상/산출물: 예정 sweeper·재시작 scan·entry 삭제 progress/오류·실제 filesystem 가용량 관측.
- 불변 조건: 삭제는 지정 trash 내부 identity에만; 완료되지 않은 삭제를 free로 표시 금지; symlink target 순회 금지.
- 시험: 정상 sweep·부분 완료 재시작; permission failure·busy file·압력 중단; 취소/재시작/새 trash 유입 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py cache-sweep`.
- 완료 증거: 재개 cursor/잔여량·실패 이유·실측 공간, 반복 실행의 보호 데이터 무변경.
- rollback: sweep 즉시 중지 가능; 이미 삭제된 재생성 cache는 복원 보장하지 않고 재빌드 경로 제공. 운영 데이터는 삭제 대상이 아니어야 한다.
- 인계: DGC-C05에 batch 비용·trash 잔량·실제 free 관측을 전달한다.

### DGC-C05 — 유지관리 부하와 공유 저장 공간

- 소유/예정 PR: DevGuard / DGC-P3. 예정 제목: `feat(cache): budget maintenance and shared filesystem capacity`.
- 문제 → 동작: reclaim 자체의 I/O/CPU 부하와 여러 root가 공유하는 APFS/container 용량을 중앙 회계에 반영한다.
- 선행: DGC-C04. 실제 filesystem/container identity·watermark·측정 권한, 낮은 우선순위 maintenance budget.
- 대상/산출물: 예정 maintenance admission·batch/pause 정책·shared pool deduplication·capacity receipt.
- 불변 조건: 여러 volume/root의 같은 backing 공간 중복 합산 금지; du 합계를 실제 free로 해석 금지; control 여유 침해 금지.
- 시험: 정상 저부하 회수; disk probe 실패·상위 lease 부족; 두 root 동시 sweep·외부 쓰기·snapshot 변화와 계측 경쟁.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py cache-capacity`.
- 완료 증거: pool identity와 전/후 실제 가용 공간, 삭제 논리 byte/물리 증가 분리, 유지관리 중 관제 지연.
- rollback: maintenance만 중단·잔여 trash 보존; 공간 부족을 해결하려 보호 class를 자동 완화하지 않는다.
- 인계: DGC-C06에 pool별 fixture와 관측 한계·동시 외부 변경 정보를 넘긴다.

### DGC-C06 — 보호·경쟁·실제 회수 qualification

- 소유/예정 PR: DevGuard / DGC-P3. 예정 제목: `test(cache): qualify deletion safety and measured space recovery`.
- 문제 → 동작: “삭제가 안전함”과 “가용 공간이 증가함”을 별개의 검증 결과로 남긴다.
- 선행: DGC-C05. 시험 전용 cache/root·고정 보존 sentinel·실제 filesystem·cold/warm fixture; 운영 cache를 지워 부하를 만들지 않는다.
- 대상/산출물: qualification harness·보호 sentinel hash·lease race transcript·실측 전후 공간 및 foreground report.
- 불변 조건: 보호 대상 GC 0건; evidence/stable artifact 생존; 공간 증가 불확실이면 효과만 inconclusive로 표시하고 안전성 결과와 구분.
- 시험: 정상 reclaim/rebuild; 중단·권한·symlink/root 교체; active lease와 mark/sweep·재시작 동시 경쟁, 부하 SLO.
- 검증 명령: 현재 DG 회귀 + 예정 `python3 scripts/qualify.py cache`; 응답성은 idle10분/부하30분 이상/3회.
- 완료 증거: 보호 hash 동일, 삭제별 권한·identity·lease 증거, 물리 free 변화와 외부 변수, 관제/foreground 원시 표본.
- rollback: 자동 회수 설정을 닫고 보존된 운영 artifact로 복귀; 재생성 cache만 재빌드. 삭제 자체의 undo를 약속하지 않는다.
- 인계: 검증된 root class·filesystem 조합에만 rollout. 다른 저장소의 cache 분류는 개별 검토하며 이 문서는 CodeSpace 외 코드를 감사한 것으로 표시하지 않는다.
