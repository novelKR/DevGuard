# 소비 저장소의 최소 도입 조건

이 문서는 특정 언어나 제품에 독립적인 도입 gate다. DG-1은 측정한 호스트에서 release `0.1.0-5daee5d-b3fa569e`로 R2와 RS를 허용하며, R3에는 여전히 CS-RG가 필요하다. 실제 코드 경로를 대조한 소비자는 CodeSpace이며 다른 저장소의 구현 적합성까지 검증한 문서가 아니다. 구현 상태의 원본은 [milestones.json](../../../milestones.json), 선택 근거는 [decisions.md](decisions.md)다.

## 도입 수준

| 수준 | 최소 진입 조건 | 허용 범위 | 다음 gate/불충족 시 처리 |
| --- | --- | --- | --- |
| R0 계약 검토·adapter 준비 | DG-0 source·계약·44개 시험 결과 | 타입/오류/상태 전이의 소비 설계 | daemon/OS 보호가 있다고 표시하지 않음 |
| R1 제한된 기능 시험 | DG1-C01~C08 실제 인증·launch·회수 경로, 격리된 시험 호스트·명시 budget | 개발 후보 기능·장애 시험 | 일반 개발 적용·SLO 자격으로 승격 금지 |
| R2 macOS 개발 적용 | DG1-C12 qualification, 실제 probe·충분한 budget, 명시 CLI/adapter 진입점 | 검증한 generic/Cargo 명령·호스트 조합의 일상 개발 | 미지원 도구/host는 별도 검증 또는 거절 |
| R3 CodeSpace macOS 런타임 | R2 + CSRG-C08, 지원 client/artifact/wire 조합 | 검증한 모드의 required 소비와 관제 보호 | 기본 off에서 자동 전환하지 않음 |
| R4 Linux 강제 보호 | 해당 Linux 환경의 DGL-C06 추가 | 검증된 자원·scope의 kernel 제어 | controller·ancestor·권한 부족이면 required 거절 |
| RS 자기 적용 | C09 보호 artifact, C10 부모 예산 기능 시험 후 동결한 부모·parent lease·격리·독립 복구; 일상 사용은 C12 | 후보 개발/시험을 기존 budget 안에서 수행 | 기능 기준과 SLO 안정 artifact의 자격을 별도 표시 |

RS는 R0~R4와 별도의 자기 적용 축이다. 최초 bootstrap은 기능 suite 후 동결한 기준 artifact로 시작하며, DG1-C12 전에 이를 제품 SLO 안정 버전으로 표시하지 않는다. DG-1은 CodeSpace runtime 통합을 요구하지 않는다. R3에서 CodeSpace 관제·승인·replay를 추가 검증한다.

## 공통 필수 조건

| Gate | 필요한 증거 | 책임/작업 | 실패 처리 |
| --- | --- | --- | --- |
| G01 실행 호스트/authority | 실제 executor identity, canonical socket/state·UID·lock, 단일 정상 authority | 운영자, DG1-C01/C02; VM은 DGA-C07 | 다른 경로에 전체 예산 authority를 만들지 않고 거절 |
| G02 실제 소비 진입점 | 실행 argv→adapter→attempt→lease→scope receipt | 소비자, DG1-C07/C08·CSRG-C03/C04 | 설정 파일만 있는 명령은 미관리로 표시 |
| G03 충분한 예산 | 유효 용량−host 여유−정적 제어 예약에 최소 작업이 들어가는 계산 | authority, DG1-C03; Linux DGL-C01 | 부족하면 거절; 강제로 1 worker 발급 금지 |
| G04 인증/자격 | OS peer와 실제 등록 owner 대응, generation, payload FD/로그 secret 비노출 | DG1-C02/C05·CSRG-C02 | unauthorized 거절; peer identity를 caller JSON으로 받지 않음 |
| G05 자원별 능력 | requested/supported/applied와 method/level·fresh evidence | DG1-C04·DGL-C02 | required 미지원/부분 적용 실패 시 실행 허용 차단 |
| G06 실행 수명 | prepare 만료·응답 유실·cancel·restart·tracking loss fixture | DG1-C05/C06·CSRG-C03/C04 | 불확실 상태 보존·자동 재실행 금지 |
| G07 독립 관제 | authority 신규 admission 실패 중 기존 handle 조회/종료 결과 | 소비 제품, CSRG-C05~C08 | 신규 작업만 거절; 조회/종료에 신규 grant 요구 금지 |
| G08 호환 조합 | source/client full SHA, 설치 daemon/helper hash, 제품 wire/capability fixture | DG1-C11·CSRG-C01/C08 | 미지원 조합 거절; 버전 문자열만으로 호환 추정 금지 |
| G09 상태/증거 보호 | journal·Git·qualification·안정/복구 artifact의 cache 제외 | 운영자, DGC-C01/C02 | 보호 분류 미완성 root의 자동 회수 금지 |
| G10 복구 가능성 | candidate 불능 상태에서 repair, drain/rollback와 원래 ledger 보존 | DG1-C09~C11; P1R-C01~C06 | 오래된 journal로 덮어쓰거나 argv 재생하지 않음 |

한 호스트의 모든 참여 소비자는 같은 정상 예산을 공유한다. 실행이 원격이면 실제 executor 호스트에서 회계/제어를 수행한다. gateway나 개발 Mac의 여유 용량을 원격 workload 용량으로 대신하지 않는다. 비참여 프로세스는 완전히 관리되는 범위가 아니므로 host pressure와 여유분을 함께 관측한다.

## 플랫폼과 보장의 범위

| 환경 | 목표 지원 | 도입 자격 | 보장할 수 없는 주장 |
| --- | --- | --- | --- |
| 현재 DG-0, 어느 OS든 | 타입·회계·가짜 backend | R0 | 실제 daemon/auth/OS cap/SLO |
| macOS native | 중앙 admission·회계, 실제 지원 QoS/priority와 관측·launch fence | R2 또는 R3 | tree-wide memory/tasks kernel cap, 전용 물리 코어 |
| Linux cgroup v2 | 실제 위임·ancestor 내 CPU/memory/pids 등 자원별 제어 | R4, 실제 host qualification | controller 존재만으로 적용 완료 또는 절대 응답 시간 |
| VM/container/원격 executor | 실제 host/guest binding과 상위 budget | DGA-C07/C08 및 필요한 R4 | Docker CLI 종료=container 종료, host+guest 독립 총량 합산 |
| 기타 OS/도구 | 별도 adapter/qualification 필요 | 현재 미지원 | generic wrapper만으로 같은 강제 수준 |

수치는 [승인 설계 §2.3](../../design.ko.md)의 초기 정책을 그대로 사용한다. CPU는 millicpu, memory는 byte다. host 여유 CPU `max(논리 CPU 25% 올림, 2 CPU)`, RAM `max(25%, 4 GiB)`, CodeSpace 제어 인스턴스 `1 CPU/512 MiB`, daemon `0.25 CPU/128 MiB`, CLI 관제 풀 합계 `0.25 CPU/128 MiB`는 **검증 전 초기값**이며 측정된 충분성으로 광고하지 않는다. CodeSpace 예약은 Gateway와 Runner 비용을 함께 포함한다.

압력으로 신규 목표를 줄여도 이미 발급한 live lease 금액을 줄이지 않는다. 측정 실패는 보호를 꺼도 된다는 근거가 아니다. 모든 지원 claim은 자원별 method/level과 실제 검증 조합을 함께 표시한다.

## 소비자 구현을 시작할 때의 계약

소비자는 프로세스 handle·입출력·승인·workspace를 소유하고 authority에는 버전이 있는 execution 의미와 resource intent를 보낸다. 새 attempt와 transport request ID를 구분한다. 변경된 의미로 같은 attempt를 쓰면 충돌하며, 종결 attempt를 새 실행으로 재사용하지 않는다.

준비 성공이 사용자 executable 성공을 뜻하지 않는다. 예약, plan, applied, helper READY, executable 실행 결과를 따로 노출한다. 준비 취소와 post-commit 취소는 회수 조건이 다르다. 루트 PID 소실·응답 timeout·연결 소실·빈 디렉터리만으로 scope 종료를 추정하지 않는다.

기존 실행의 조회·종료는 owner의 handle과 제어 여유로 수행한다. authority가 고장 나도 관리 프로세스를 멈추는 경로를 남긴다. 재연결은 기존 실행에 대한 관측/제어권 복구이며 새 workload budget을 발급하는 행위가 아니다.

## 호환성·운영 handoff

| 독립 축 | 기록할 값 | 필수 시험 |
| --- | --- | --- |
| source/client | 전체 SHA, dependency graph/lock, 소비 adapter revision | 타입/오류·strict decoding·capability negotiation |
| daemon/helper | 실제 binary hash, host/arch, policy/journal schema | 실행 단계·FD·N/N+1 read/write·upgrade/repair |
| 소비 제품 wire | CodeSpace 등 자체 protocol version와 mode | 준비/실행/관제/replay/복구 및 구·신 조합 |

현재 serde의 `deny_unknown_fields` 때문에 단순 필드 추가도 실제 호환 시험 대상이다. schema가 달라지면 migration·downgrade 거절 조건을 함께 제공한다. 운영자가 artifact와 소스 pin을 맞추고 최초 adoption report를 보존한 뒤 검증 범위만 활성화한다.

rollback은 신규 admission 차단 → 살아 있는 작업 관측/정리 → ledger 대조 → 호환 artifact/설정 복귀 순서다. 후보의 admission이 repair 선행 조건이 되어서는 안 된다. [CodeSpace 명세](codespace-integration.md), [검증 규칙](verification.md), [PR 절차](pr-delivery.md)를 해당 제품의 도입 기록에 연결한다.

C10의 기능 확인 직후 실제 bounded 자기 적용을 시작한다. C08/C09 artifact가 새 부모 기능을 지원한다고 가정하지 않고 C10 기능을 포함한 부모를 먼저 시험·동결한다. 이후 해당 build/test를 부모로 관리하며 C12 전에 SLO 릴리스라고 표시하지 않는다.
