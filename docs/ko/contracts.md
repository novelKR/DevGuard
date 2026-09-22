# 구현된 authority 계약

이 문서는 DG-0 authority와 C01 서비스·저장소 경계를 설명한다. 실제 제공 명령은 [운영 문서](operations.md)에 있다. Native launch, OS 정책 적용과 CodeSpace 결합은 아직 구현하지 않았다. [영문 정본](../contracts.md).

## Authority와 transport 경계

Core는 Rust 라이브러리다. Authority::register는 후속 transport가 OS peer 자격을 관측하여 만든 TrustedPeer를 받고, 설정 UID·소비자 자격·generation·정확한 프로세스 정체성을 Backend로 검증한다. 성공하면 불투명 Principal을 반환하며 workload는 공개 API로 control-service Principal을 만들 수 없다. Workload 소비자는 제어 예약을 설정할 수 없다. 관리 대조와 generation 폐기는 신뢰하는 daemon·운영자 경로의 책임이며 workload RPC로 공개하면 안 된다.

DG-0는 가짜 peer·backend로 이 라이브러리 경계를 검증한다. UDS 인증·자격 FD 전달·동일 UID 공격자 격리·network client의 구현 증거가 아니다. DG-1은 실제 경계를 제공한 뒤에만 운영 서비스로 표시해야 한다.

Authority는 journal 부모 디렉터리에 no-follow 배타 잠금을 보유한다. 같은 디렉터리의 모든 journal은 그 잠금을 공유한다. C01은 caller HOME·XDG가 아니라 OS 계정으로 정상 경로를 결정하고 state·socket override를 거절한다. 프로젝트 설정에는 authority 자격이나 호스트 용량을 둘 수 없다. 운영 후보 경로는 C10의 부모 lease 경계가 제공될 때까지 사용할 수 없다.

AuthorityStorage는 boot clock을 만들거나 attempt를 복구하거나 capability를 부여하지 않고 journal의 배타 열기·검사만 수행한다. Authority::from_storage는 실제 Backend·Clock으로 활성화하며 복구 transaction 안에서 회계 인덱스를 다시 검증한다. 기존 Authority::open도 같은 경로로 기존 복구 동작을 유지한다. 명시 bootstrap과 일반 open은 별개이며 누락·손상·미래 schema를 자동 수리하지 않는다.

## 내구성 admission과 launch

Journal schema는 1이다. 초기화는 명시적으로 새 파일만 만든다. 누락·손상·미지원 schema·불일치 journal은 fail-closed다. SQLite는 WAL, FULL synchronous와 immediate transaction을 사용한다. 시작 시 모든 저장 record와 회계 index를 대조한다.

Attempt key는 `(consumer_id, consumer_generation, attempt_id)`다. 요청 fingerprint는 버전 있는 실행 digest와 자원 intent를 포함하고 transport ID·현재 policy revision을 제외한다. Replay는 최초 내구 예약·종결 결과를 반환하고 의미나 owner 변경은 충돌한다. 거절도 종결 attempt이므로 나중에 별도로 요청하는 admission은 새 attempt ID를 사용한다.

begin_launch는 Prepared를 내구성 있게 소비하고 일회성 Secret permit을 반환한다. 반복 호출에는 새 spawn 권한이 없고 기존 attempt만 있다. Journal에는 permit digest만 저장한다. 첫 응답이 유실되면 대조해야 하며 transition replay로 helper를 다시 만들 수 없다.

bind_scope는 attempt·owner·정확한 프로세스 정체성·scope·전체 적용 plan을 연결한 신선한 Backend 증거를 요구한다. authorize_run은 연결된 helper 정체성과 permit을 검증하며 최초 성공 응답만 may_exec=true다. 이 응답 유실은 불확실·차단된 실행이지 재생성 허용이 아니다. RunAuthorized는 사용자 executable 성공의 증거가 아니다.

Prepared 취소는 종결하고 예약을 반환한다. Commit 후 취소는 Draining으로 전환하고 늦은 bind·authorize를 막으며 예약을 유지한다. Prepared만 5초 후 만료된다. 재시작은 Prepared의 원래 boot-relative 기한을 유지하고 commit 후 미종결 attempt와 등록 instance를 Suspect로 전환하여 대조한다.

## 회수 증거

무조건 release(lease_id) API는 없다. Bound 실행은 정확한 scope와 신선한 root 종료·reap, scope 비움, 알려진 구성원 생존 없음, 완전한 추적과 알려진 이탈 없음이 필요하다. 이전 추적 상실은 평범한 빈 group 관측만으로 사라지지 않는다. 명시 대조 증거나 검증된 호스트 reboot 종료가 필요하다.

Unbound committed launch는 PID 누락이나 owner 사망만으로 회수하지 않는다. Backend가 helper 미생성과 모든 pending spawn 부재를 적극적으로 입증해야 한다. Reboot로 이전 실행 종료를 대조할 수도 있다. release_reason은 scope 종료·helper 미생성·이전 boot 종료를 구분한다. known_not_started는 prelaunch 종결 거절과 Released/NoHelperCreated에서만 참이다. 자원 반환 자체는 재시도 증거가 아니다.

일반 TTL sweep은 tombstone을 삭제하지 않는다. 운영자 generation 폐기에는 모든 instance retired와 charge 없음이 필요하다. 종결 attempt 압축 전에 영구 retired generation을 기록하여 과거 key를 계속 거절한다.

정적 제어 예약은 단절·offline을 포함한 모든 설정 슬롯에서 차감한다. Instance retire는 슬롯 재사용만 허용하고 정적 예약을 workload로 반환하지 않는다. 프로세스 정체성에는 boot ID·PID·start ticks가 있어 PID 재사용을 검출한다.

Journal은 instance의 등록 정책을 기록한다. Active/suspect instance가 남은 동안 소비자 삭제나 generation·role·UID·instance 상한·예약 변경은 시작 시 거절한다. 기존 설정에서 먼저 대조·retire한다. 슬롯 수는 generation을 가로질러 합산하므로 설정 재시작으로 같은 정적 예약에 두 번째 서비스를 배정하지 못한다.

## 압력과 capability

최초 유효 sample 전에는 closed다. 최초 정상 sample은 준비를 열지만 압력·관측 실패 후에는 설계의 30초 단계 복귀를 따른다. 같은 boot-relative monotonic clock을 사용하고 replay·미래 timestamp를 거절하며 6초가 지나면 stale이다. 목표 감소로 live lease 금액을 줄이지 않는다.

자원마다 level·method가 있다. Accounting은 OS 메모리 상한이 아니고 QoS는 메모리·task 제한이 아니며 kernel 제어에는 contained cgroup이 필요하다. Admission 전에 plan을 검증하고 AppliedResources와 구분한다. 호환성은 protocol과 모든 필수 capability를 함께 확인하며 fallback authority를 시작하지 않는다.

## 후속 마일스톤의 책임

- 남은 DG-1: host probe, 실제 UDS 자격·framing, helper, 실행 CLI, Cargo, bounded 자기 적용, update·repair, 측정한 macOS SLO.
- CS-RG: Runner 슬롯·전송 lane, 승인 migration, client pin, 상태 결합과 회귀 qualification.
- DG-LINUX: 실제 cgroup 계층·controller·ancestor와 sandbox·proxy 포함.
- DG-CACHE·DG-ADAPTERS: 등록 cache 회수와 추가 도구 제어.

Journal·라이브러리는 사용자 프로세스 handle·출력·CodeSpace workspace lease·approval row를 소유하지 않는다. 자원 예약 수명으로 이들의 수명을 추정하지 않는다.
