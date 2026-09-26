# DevGuard 설계 참조

기준일: 2026-09-22, [설계 개정 1](design-revision-1.md)로 2026-09-27 개정. 로컬 소스: `/Volumes/DevData/Projects/IdeaProjects/DevGuard`.
이 문서는 [영문 편집 정본](../design.md)의 관리되는 한국어 대응 문서다. 전체 승인 원문 [design.ko.md](../design.ko.md)와 [checksum](../design-source.json)은 byte 그대로 보존한다. 이 참조 문서는 승인 설계, 후속 [결정](planning/decisions.md), [개정 1](design-revision-1.md)처럼 사용자가 지시한 설계 개정을 종합하며 원래 승인 artifact라고 주장하지 않는다. 역사적 승인본은 당시 결정을 보존하고 이 참조는 현재 편집 기준이며, 마일스톤 상태는 실제 구현·검증에 따라서만 바뀐다. 구체 작업은 [계획](planning/README.md), 현재 구현 사실은 [계약](../contracts.md)이 소유한다.

## 목적과 신뢰 범위

실제 실행 호스트의 한 authority가 여러 저장소의 개발 자원을 합산한다. CodeSpace는 예산·정책을 소비하고 인가·승인·workspace·프로세스 handle·PTY·입출력·timeout·종료를 계속 소유한다. DevGuard 후보 개발도 검증된 부모 예산 안에서 수행한다.

회계상 제어 예약, 실제 OS 제어 적용, 측정된 관제·foreground SLO는 별개다. macOS 회계만으로 트리 전체 kernel 상한이나 응답성을 입증하지 않는다. 최초 신뢰 범위는 한 운영 계정의 등록된 협조적 workload이며 악의적인 동일 UID·다른 UID·미등록 앱의 강제 격리를 제공하지 않는다. 외부 부하는 host pressure에 반영한다. 알려진 process group 이탈·정체성 불일치·추적 실패는 Suspect다. 설치 SDK가 미지원으로 표시한 macOS NOTE_TRACK을 완전한 자손 추적의 근거로 사용하지 않는다.

## 책임과 정체성

daemon은 용량·정적 예약·admission·lease·압력과 후속 cache 정책을 소유한다. launcher는 payload 전 scope·정책을 확인하며 범용 프로세스 서버가 아니다. CLI는 직접 시작한 명령을 관제한다. contract/client·core·native backend·launcher·daemon/CLI·언어 adapter를 분리하고 core에 제품 타입을 넣지 않는다. contract·core·범용 client에는 Codex 제품 타입·모델 세션·CodeSpace workspace 권한·PTY 소유권을 넣지 않는다. DevGuard의 기본 배포와 공용 client는 현재 CodeSpace/Codex 없이 빌드·시험·릴리스하며 CodeSpace가 작은 client를 전체 SHA로 고정해 소비한다. 이는 영구 금지가 아니라 현재의 공학적 선택이다. 실행·플랫폼 adapter의 저수준 유틸리티 재사용은 실제로 대체하는 코드·계약 적합성·의존성 전파·복구 경로·재검증 비용으로 결정한다([개정 1, D3](design-revision-1.md#103-d3--devguard-의존성-정책)).

운영 계정·executor 호스트마다 canonical state와 배타 소유권을 가진 정상 authority 하나만 둔다. 다른 socket/state 경로로 정상 전체 예산을 추가할 수 없다. 원격 worker는 실제 실행 호스트의 authority를 소비하며 host와 guest의 용량을 독립 여유분으로 합산하지 않는다.

consumer_id는 운영자 등록, instance_id는 PID/start/boot, attempt_id는 transport와 독립된 실행 시도다. 최대 인스턴스 수×정적 제어 예약은 연결이 끊겨도 작업 예산에서 제외한다. active/suspect/retired 인스턴스의 대조 후 슬롯을 재사용한다.

소켓 부모0700·소켓0600, OS peer UID/PID와 역할·generation 자격을 검증한다. UID만으로 control_service를 허용하지 않는다. 등록 자격·일회성 helper permit·관리 권한을 구분한다. private FD 자격은 payload 전에 닫고 argv/env/debug/journal에 남기지 않는다. 프로젝트 설정은 정책을 강화할 수만 있고 용량·역할을 높이거나 소비자를 사칭할 수 없다.

CodeSpace의 단일 등록자는 실행 소유자인 Runner다. InProcess는 Gateway PID, UDS는 worker PID다. Gateway와 Runner 비용은 정적 예약 하나에 포함한다. 별도 service-exec 경로는 없다. Gateway는 소비자 자격을 `CredentialHandoff`로 UDS worker에 넘기고 InProcess는 직접 읽으며, 한정된 세션마다 같은 instance를 다시 등록한다. 서비스/하위 worker 등록은 실제 다중 Runner 수요 때 재검토한다.

## 예산과 압력

CPU는 정수 millicpu(논리 CPU당1000), 메모리는 byte다. 초기 정책값은 측정된 충분성과 구분한다.

| 항목 | 초기값 |
| --- | --- |
| 호스트 CPU 여유 | max(논리 CPU25% 올림,2CPU) |
| 메모리 여유 | max(물리 RAM25%,4GiB) |
| 설정된 CodeSpace 제어 인스턴스 | 1CPU/512MiB, 최초 최대1개 |
| daemon | 0.25CPU/128MiB |
| CLI 관제 풀 | 총0.25CPU/128MiB, 최대8개 CLI |
| 작업 | macOS Utility QoS,nice+10 |
| 후속 GC | Background QoS,낮은 I/O 우선순위,유한 batch |
| 관측/신선도 | 2초 간격/최대6초 |
| admission/Prepared | 250ms/5초 |

기본 작업 예산=유효 용량−호스트 여유−정적 예약. 신규 허용량=max(0,압력 목표−미회수 lease). Linux는 ancestor/controller 제약도 반영한다. 최소 작업이 안 들어가면 거절하며 강제로1worker를 만들지 않는다. 실측 초과는 기록하고 신규 허용을 제한하지만 RSS 감소로 살아 있는 예약을 반환하지 않는다.

Normal은 기본 예산이다. Constrained는 memory warning,10초 page-out평균≥16MiB/s,10초 swap증가≥64MiB,관제 loop지연≥200ms 중 하나가 연속2회일 때50%목표다. Linux memory PSI full avg10≥2%도 포함한다. Critical은 memory critical,대상 볼륨 여유≤max(5%,2GiB),관측6초 초과 지연 또는 loop지연≥1초/Linux PSI≥10%의 연속2회에서 신규 heavy작업을 막는다.

정상 메모리·수치가 진입 절반 미만·신선한 관측30초·disk>max(10%,4GiB)가 복귀 조건이며 한 단계씩 회복한다. 높은 CPU만으로 Critical로 만들지 않는다. 목표 감소로 기존 lease를 줄이거나 실행 중 Cargo jobs를 바꾸거나 임의 명령을 종료·재실행하지 않는다. [Linux PSI](https://docs.kernel.org/accounting/psi.html)는 수단이며 이 임계값은 DevGuard 검증 대상이다.

## 자원과 내구성 실행 계약

ResourceIntent·Reservation·ExecutionPlan·AppliedResources·실행 결과를 분리한다. CPU/메모리/tasks마다 요구·예약량,방법,scope,최소 수준(accounted/cooperative/kernel),적용 상태,backend·시각을 보고한다. macOS 기본은 CPU cooperative,메모리/tasks accounted이며 kernel 요구는 실행 전에 거절한다. QoS/nice는 트리 전체 강제 상한이나 독점 코어가 아니다. 후속 Linux는 위임된 cgroup을 실제 적용/readback하며 cpu.max는 독점 코어,memory.min은 물리 선할당이 아니다. [Apple QoS](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/PrioritizeWorkWithQoS.html),[cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html).

멱등성 키는(consumer_id,consumer_generation,attempt_id)다. 버전 있는 의미 digest는 executable/cwd정체성·argv·허용 env변경·TTY·timeout·intent를 포함하며 journal에는 원문 대신 digest/회계만 저장한다. 같은 키·의미는 최초 결과/정책,다른 의미는 충돌,종결 키는 그대로,응답 유실은 같은 키다. 미시작 확정 후에만 명시적 새 시도를 만들며 불확실 실행은 보수 회계와 자동 재실행 금지를 유지한다.

SQLite transaction은 예약·launch commit·실행 허용 응답 전에 각각 내구 기록한다. 재시작은 관계를 복원한다. 대조 완료 generation의 명시 폐기 전까지 tombstone을 보존하고 폐기 generation을 계속 거절한다. 원장 누락/손상/full은 신규 실행 차단이며 자동 빈 원장 초기화가 아니다. 모든 장애에서 OS 실행 exactly-once를 무조건 보장한다는 뜻은 아니다.

Prepared→LaunchCommitted→helper 생성→ScopeBound/PoliciesApplied→RunAuthorized→READY→user exec시도→Draining→Released 순서다. 일회성 permit은 owner/attempt에 묶고 replay로 재발급하지 않는다. 허용 응답 유실은 unknown이다. READY나 CodeSpace의 추적 helper confirmed는 사용자 exec성공이 아니다.

Prepared만 원래5초 기한에 만료된다. 취소/만료는 늦은 commit을 원자적으로 막고 반환한다. commit후에는 실제 미생성/종료 증거가 필요하다. spawn전 실행 슬롯,resource lease,완료 출력 보존은 별도 수명이다. root reap·연결/응답 소실·scope누락만으로 전체를 회수하지 않는다.

macOS는 협조적 workload가 group을 이탈하지 않는 전제에서 root reap·관측 group비움·알려진 구성원 종료·미해결 추적 오류 없음이 필요하다. PID/start/boot로 재사용을 검증한다. 후속 Linux는 sandbox/proxy포함 cgroup populated=0과 helper종료가 필요하다. reboot는 이전 boot종료 증거지만 journal초기화 이유는 아니다. 빈 scope가 page-cache/disk즉시 회수를 뜻하지 않는다.

## Adapter와 후속 cache

최초 generic/Cargo adapter를 제공한다. generic은 병렬도 변환 미지원을 표시한다. Cargo직접/명시 cargo-pipeline은 명령 의미·target/report·유효 jobserver FD를 보존한다. jobs는 CPU와512MiB고정+compiler job당1.5GiB 추정 중 작은 한도에 맞춘다. explicit -j/--jobs/env우선순위·조정/거절 이유와 중첩 token소유권을 보존한다. 임의 shell문자열을 다시 쓰거나 Cargo jobs가 모든 test스레드를 제한한다고 주장하지 않는다. 언어별 env처리는 adapter책임이다. [Cargo 설정](https://doc.rust-lang.org/cargo/reference/config.html).

Python/JS/make/ninja/VM/container/학습 추정은 후속이다. Docker CLI제한은 실제 executor제어와 다르다. cache는 등록 root/class와 사용 배제가 필요하다. Disposable/저비용 재생성은 조건부,고비용은 보존 정책,환경/영속 artifact는 기본 보호다. Git/journal/evidence/설치·복구 binary/CodeSpace target/upstream-reports/local을 보호한다.

GC는 사용/reclaim배제→내구 mark→동일filesystem trash rename→중단 가능한 sweep→실제 여유 측정이다. identity/symlink이탈을 검사하고 trash는 재시작 후에도 점유다. 비활성7일·목표min(저장공간8%,50GiB)는 적격 cache만의 미검증 초기값이다. APFS du는 물리 회수량이 아니다. CARGO_TARGET_DIR변경/sccache활성화는 자동 수행하지 않는다.

## 실행 소유권(개정 1)

[설계 개정 1](design-revision-1.md)은 정확성 목표를 유지하고 그 구현 방식을 재평가한다. 목표는 프로세스마다 회수 책임자 하나, 관리 실행의 회수 전 관찰 기회 보존, permit·자격·transcript descriptor를 무관한 실행이나 payload에 넘기지 않음, 응답 유실·timeout·EOF·root 회수만으로 미실행이나 scope 전체 종료를 확정하지 않음, 준비의 일회성 소비와 불확실 실행의 자동 재실행 금지, 신규 허가 실패가 조회·종료를 막지 않음, 종료·출력 정리·workspace 해제·lease 반환의 구분, 구현·기능·플랫폼·SLO 증거의 분리 기록이다.

CodeSpace는 실행 identity·승인 연결·상태·timeout·종료 요청·출력·반환을 한 계층에서 조정하고, PTY·pipe·자원 관리 여부의 차이를 OS child 생성·입출력 연결·terminal 설정·종료 관찰·실제 회수라는 좁은 backend 경계에 가둔다. 스스로 회수하는 backend(`BackendReaped`, 초기 legacy `off` 경로)는 회수되지 않은 종료 상태를 제공한다고 표시하지 않는다. `required` 경로는 `OwnerControlledReap`으로, child를 소유한 객체 하나를 통해 회수 전에 관찰한다. 준비 결과는 소유권 있는 일회성 객체이며 복제하거나 argv로 다시 만들지 않는다.

- **D1.** `required` 실행의 기본안은 현재 pin에서 가능한 CodeSpace 소유 Unix transport다. 새 코드를 쓰기 전에 공개 API, 같은 계약의 upstream 후보, 출처를 기록한 제한적 adaptation(A4), 자체 구현 순으로 재사용 가능성과 계약 차이를 기록한다. Codex `ProcessDriver`는 출력 손실·backpressure·Drop 기준을 만족할 때만 채택한다.
- **D2.** CS-RG 최종 qualification 전에 legacy `off` backend를 통합해 제거하거나, 기록된 근거로 제한적 compatibility backend로 유지한다.
- **D3.** [책임과 정체성](#책임과-정체성)을 따른다.

개정 1은 Codex pin을 유지하며 변경은 검증에 근거한 별도 결정이다. adaptation 정책과 재검토 트리거는 [ADR-006](planning/decisions.md#adr-006--codespace-실행-소유권과-재사용-정책)에 있다.

## 소비와 복구

[CodeSpace 결합](planning/codespace-integration.md)이 고정 source경로·오류·흐름을 소유한다. 개정 1은 Codex pin을 유지하며 이후 변경은 검증에 근거한 별도 결정이다. source/client SHA,설치 daemon/helper hash,제품 wire/capability는 독립 축이다. 엄격한 decoding은 구신 fixture를 요구하며 필드 추가가 자동 호환은 아니다. 기본off,required의 자동 완화 금지다.

기존 인가/workspace FIFO후 spawn전 슬롯·PrepareExec를 확보하고250ms내 빠르게 거절한다. 장기 제품 대기열은 없다. 준비 예산 250ms, Prepared 수명 5초, DG-1의 frame별 250ms 기한은 서로 다른 제한이다. 준비 성공 후 동일 attempt승인 CAS와 ExecPrepared가 진행한다. 미시작/confirmed/READY/exec실패/unknown을 구분한다. 예산 부족·관제서비스 장애·미지원 정책 오류를 구분하고 terminate_process·기존 workspace오류·patch원장 경계를 보존한다.

control/data는 처리·전송 여유와 전체 queue/bytes/retention을 분리·제한하며 lock/writer/callback도 포함한다. 작업동시8·대기64와 별도 control여유가 초기안이다. inflight replay는 같은 dispatch를 공유하고 eviction은 재실행 허용이 아니다. authority장애에도 기존 handle관제는 유지한다.

P1은 생존 독립 Runner에 대한 opt-in Gateway복구다. 기존 종료 계약을 보존하고 정상 종료/명시stop/detach/예상치못한 단절을 구분한다. 원래timeout/I/O를 Runner가 유지하고 인증 Gateway는 epoch/fence와 workspace/approval/lease대조로 같은 실행에 재연결하며 새 예산을 요구하지 않는다. InProcess·Runner재시작후I/O복원은 제외한다. Runner/host손실은 unknown이며 argv를 재실행하지 않는다.

## 경로·bootstrap·운영

Rust1.95.0으로 검증한다. devguard/devguardd/devguard-launch/작은 client를 제공한다. macOS설정은 ~/.config/devguard/host.toml,분리된 state/releases/recovery/evidence는 ~/Library/Application Support/DevGuard/,짧은socket은 /private/tmp/devguard-<uid>/,cache는 ~/Library/Caches/DevGuard/다. 소유권·권한·symlink를 검사하고 lock/journal은 tmp에 두지 않는다. 프로젝트.devguard.toml은 프로젝트/profile/adapter/더 낮은 상한만 지정한다.

예정명령 devguard daemon serve,doctor,exec --wait,test-candidate,upgrade,repair는 해당 작업에서 구현된 뒤 제공된다. CLI대기는 이전 미시작이 확정돼야 새attempt를 만들며 장애의 자동 비관리fallback은 없다.

C08까지 foreground daemon과 최소 단일Cargo job/시험thread bootstrap을 사용한다. P4기능 bundle은 target밖에 보존한다. C09는 보호 artifact와 현재 사용자의 LaunchAgent를 설치한다. 같은 foreground진입점을 쓰며 특권 system서비스가 아니다. 수명은 로그인 사용자에 따른다. [Apple launchd](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).

C10에서 새 부모예산 기능을 먼저 시험하고 이를 포함한 부모를 동결한 직후 실제 bounded자가적용을 시작한다. 이전 artifact가 새기능을 이미 지원한다고 가정하지 않는다. 후보용량≤부모lease,격리 state/socket/자격/cache,실제workload의 stable-launcher중재를 요구한다. 정상 두번째예산·제어자격·운영cache접근은 금지한다. 이후 해당 build/test를 부모로 관리하고 receipt를 보존한다. 기능부모와 SLO릴리스를 구분하며 C11복구는 고장난 후보admission과 독립이다.

Upgrade는 호환검사→admission닫기→Prepared취소→active/suspect drain대조→quiescent backup→artifact교체→state/handshake확인→재개다. 기본60초drain실패는 교체중단이며 charge삭제가 아니다. 재개전실패는 quiescent backup복귀가능,새admission후 과거state복원은 금지한다. Repair는 보호된 호환artifact와 배타authority로 독립실행한다. 미대조 손상state는closed,비호환downgrade는거절한다.

## 검증과 승격

정책초기값·가설·측정을 구분한다. 조합마다 idle10분+부하최소30분을3회,긴명령은 완료까지 관측한다. 고정sourceCargo·다중소비자·bounded CPU/memory/I/O·출력압력·느린입력을 포함하고 cold/warm시험디렉터리 및 local/remote시간을 분리한다.

압력연결소실·중복·보호GC·unknown자동재실행0,localstatus p99≤500ms,종료ack p99≤1초(실제종료별도),foreground입력→paint p99≤100ms/1초초과0,frame500ms초과0이 목표다. 전구간visibility/focus를 검증하고 무효조건/기준선실패는inconclusive다. raw latency/pressure/jobs/peak/throughput/refusal/lifecycle과 source/artifact/policy/환경hash를 보존한다.

C12는 실제8논리CPU/16GiB macOS의 standalone관제·개발·자가적용을 검증한다. 실제CodeSpace MCP/approval/replay는CS-RG이며 fakeLinux는kernel자격이 아니다. 측정한조합만승격하고 사용자서비스가 이를실행함을확인한다.

우선경로DG-0→DG-1→CS-RG→P1-RECOVERY,전체제품에Linux필수,cache/추가adapter는P1선행아니다. 구현·플랫폼자격·설계승인은별도상태다. [검증](planning/verification.md)과[전달](planning/pr-delivery.md)은 증거·정확한head순차병합·정리를규정하며 운영/reference/recovery자료를보호한다.
