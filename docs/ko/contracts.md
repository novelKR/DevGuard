# 구현된 authority 계약

이 문서는 구현된 DG-0 authority, C01/C02 서비스·저장소·transport 동작과 C03 native macOS 호스트 증거를 설명한다. PR과 병합 후 main 전달 증거는 구현과 별도로 추적한다. 실제 제공 명령은 [운영 문서](operations.md)에 있다. Wire 등록·launch, OS 정책 적용과 CodeSpace 결합은 아직 구현하지 않았다. [영문 정본](../contracts.md).

## Authority와 transport 경계

Core는 Rust 라이브러리다. Authority::register는 TrustedPeer를 받고, 설정 UID·소비자 자격·generation·정확한 프로세스 정체성을 Backend로 검증한다. 성공하면 불투명 Principal을 반환하며 workload는 공개 API로 control-service Principal을 만들 수 없다. Workload 소비자는 제어 예약을 설정할 수 없다. 관리 대조와 generation 폐기는 신뢰하는 daemon·운영자 경로의 책임이며 workload RPC로 공개하면 안 된다.

DG-0는 가짜 peer·backend로 라이브러리 등록 경계를 검증한다. C02는 실제 로컬 UDS 인증과 작은 client를 제공하지만 native 등록은 활성화하지 않는다. 서버는 OS socket 자격으로 peer UID/PID를 관측하고, client는 handshake의 authority UID/PID와 자기 정체성을 독립적으로 대조한다. 호출자가 선언한 peer 정체성을 신뢰하지 않는다. C03은 native boot/start 정체성을 추가하므로, OS가 관측한 peer UID/PID와 kernel start 정체성을 결합하면 완전한 신뢰 등록 관측이 된다. Native 시험은 이 경로를 프로세스 안에서 검증한다. 서비스는 DG1-P3가 launch와 대조를 설치할 때까지 wire 등록을 계속 거절한다. 인증만으로 Principal·instance 슬롯·lease·호스트 예산을 발급하지 않는다. 현재 workload/control-service 등록은 ResourceControlUnavailable을 반환하고 관리 자격으로는 등록할 수 없다.

Handshake는 contract 호환성과 wire version 1을 확인하며 C02는 runtime capability를 광고하지 않는다. Consumer generation과 자격 digest가 workload/control-service 역할을 결정하며 별도 관리 digest는 명시적으로 공개한 관리 역할에만 사용한다. 모든 consumer·관리 digest는 서로 달라야 한다. Caller 자격, 후속 일회성 helper permit과 관리 작업은 별도 경계이며 helper 자격 variant를 caller 인증으로 받지 않는다. 이는 협조적인 운영 계정 모델이며 악의적인 동일 UID 프로세스를 격리하지 않는다.

Authority는 journal 부모 디렉터리에 no-follow 배타 잠금을 보유한다. 같은 디렉터리의 모든 journal은 그 잠금을 공유한다. 잠금은 O_NONBLOCK으로 열고 열린 descriptor가 현재 UID 소유의 private 일반 파일이며 링크가 정확히 하나인지 명시 초기화 때도 확인한다. FIFO·링크된 파일은 잠금 대신 사용할 수 없다. C01은 caller HOME·XDG가 아니라 OS 계정으로 정상 경로를 결정하고 state·socket override를 거절한다. 프로젝트 설정에는 authority 자격이나 호스트 용량을 둘 수 없다. 운영 후보 경로는 C10의 부모 lease 경계가 제공될 때까지 사용할 수 없다.

AuthorityStorage는 boot clock을 만들거나 attempt를 복구하거나 capability를 부여하지 않고 journal의 배타 열기·검사만 수행한다. Authority::from_storage는 실제 Backend·Clock으로 활성화하며 복구 transaction 안에서 회계 인덱스를 다시 검증한다. 기존 Authority::open도 같은 경로로 기존 복구 동작을 유지한다. 명시 bootstrap과 일반 open은 별개이며 누락·손상·미래 schema를 자동 수리하지 않는다.

## 제한된 로컬 protocol과 자격 전달

Frame은 4-byte 길이와 최대 64 KiB JSON payload로 구성한다. Frame·message variant·중첩 wire type은 알 수 없는 필드를 거절하고 version·request identity·필수 capability는 따로 확인한다. 필드 추가를 자동 하위 호환으로 취급하지 않는다. 서버의 활성 session worker는 최대 32개다. 각 frame read/write의 절대 기한은 250 ms이며 다음 frame을 기다리는 idle 시간도 포함한다. Byte를 더 받아도 기한은 연장하지 않고 idle 기한을 넘긴 세션은 닫는다. 이 transport 경계가 후속 end-to-end admission 예산이나 C12 응답성 qualification을 입증하지는 않는다.

Framing은 poll·descriptor O_NONBLOCK·호출별 nonblocking socket I/O를 사용한다. Darwin에서는 호출별 flag만으로 큰 write가 제한되지 않을 수 있으므로 descriptor nonblocking도 적용한다. Peer 종료 후 Darwin timeout 옵션 변경이 EINVAL로 실패할 수 있어 옵션을 바꾸지 않고 버퍼에 남은 마지막 데이터를 읽는다. 잘못되거나 잘린 응답, 만료·통신 장애에서 실행이나 회수를 추정하지 않는다. Client는 자동 재시도나 비관리 authority·실행 fallback을 하지 않는다.

CredentialHandoff는 전용 상속 descriptor로 caller secret 하나를 전달하고 부모의 복사본에는 close-on-exec을 유지한다. take_inherited/read_owned는 제한된 길이와 250 ms 읽기 기한을 적용하며 성공·실패 모두 receiver descriptor를 소비하고 닫는다. Secret은 로컬 인증 교환을 위해 명시 직렬화하고 debug·파서 오류에서는 정제한다. Subprocess 시험은 후속 exec 전에 FD가 닫히고 argv·환경·출력에 secret이 없음을 관측한다. 이는 transport 위생 검증이며 C05 helper 권한·READY·사용자 프로그램 시작·격리의 qualification이 아니다.

## Native macOS 호스트 증거

`devguard-macos`는 실행 중인 호스트에서 core의 `Clock`·`Backend` 입력을 제공하며 core는 이 trait으로만 받는다. 다른 플랫폼에서 열면 `ResourcePolicyUnsupported`를 반환하며 이는 macOS 관측 실패(`ResourceControlUnavailable`)와 구분된다. 두 경우 모두 서비스는 저장소를 닫힌 상태로 유지하고 어느 쪽인지 밝힌다. 관측한 호스트로 유효한 정책을 만들 수 없거나 journal의 boot 인지 복구가 실패하면 `serve`는 대신 시작을 거절한다.

- **Boot 기준 시간.** `boot_id`는 프로세스마다 한 번 읽는 `kern.bootsessionuuid`다. `monotonic_ms`는 sleep 중에도 증가하고 달력 조정의 영향을 받지 않는 `CLOCK_MONOTONIC_RAW`(mach continuous time)다. 다른 boot의 관측과는 비교하지 않는다. 시작 검사 후 시계가 실패하면 시간을 지어내지 않고 프로세스를 중단한다.
- **프로세스 정체성.** `start_ticks`는 `proc_pid_rusage`의 `ri_proc_start_abstime`이며 mach absolute time 단위다. `exec`로 바뀌지 않고 재사용된 PID는 다른 값을 가진다. 각 읽기는 프로세스 표 snapshot 앞뒤로 start를 두 번 읽고, 그 사이 PID가 바뀌면 다시 읽는다. 살아 있는 프로세스만 정체성을 가지며 zombie와 reap된 PID는 모두 부재로 본다. 다른 사용자의 프로세스처럼 관측이 거부되면 오류이며 부재로 취급하지 않는다.
- **용량과 정책.** 용량은 `hw.logicalcpu`와 `hw.memsize`다. 호스트 headroom은 max(논리 CPU의 25% 올림, 2 CPU)와 max(메모리의 25%, 4 GiB)에 운영자 `additional_headroom`을 더한다. System 예약은 0.5 CPU·256 MiB(daemon과 CLI pool 합계)와 설정한 `system_tasks`다. 8-CPU/16-GiB 대상에서는 작업 가용량이 5,500 mCPU, 11.75 GiB, 144 tasks다. 이는 회계 수량이며 kernel 제한이 아니다.
- **호스트 압력.** 서비스는 2초마다 네 가지를 읽는다.
  - `kern.memorystatus_vm_pressure_level` (1 normal, 2 warning, 4 critical; 그 밖의 값이면 읽기 실패)
  - `host_statistics64`의 page-out (`pageouts + swapouts`에 `host_page_size`가 알려 주는 kernel page 크기를 곱함)
  - `vm.swapusage`
  - 상태 볼륨과 등록된 모든 project root에 대한 `statfs`

  비율은 최근 10초의 읽기로 계산하고 올림하므로 절사 때문에 임계값을 놓치지 않는다. 시작 직후처럼 창이 짧으면 swap 증가량을 10초 기준으로 외삽한다. 관제 loop 지연은 sampler가 예정보다 늦게 깨어난 시간과 직전 읽기가 일정을 넘긴 시간 중 큰 값이며, 각 읽기는 호스트 읽기에 걸린 시간(`read_ms`)도 기록한다. 예정보다 늦어지면 몰아서 따라잡지 않고 완료된 읽기로부터 한 주기 뒤에 다시 시작하며, 직전 읽기와 같은 millisecond의 읽기는 실패가 아니라 비율 없음으로 처리한다. 볼륨이 여럿이면 디스크 임계값이 가장 엄격하게 판정하는 볼륨을 쓴다. macOS에는 Linux 메모리 PSI가 없다.
- **준비 상태.** Sample에는 이전 읽기가 필요하므로 두 번째 유효 읽기 전까지 admission은 닫혀 있다. 실패하거나 일관되지 않은 읽기는 `Authority::pressure_observation_failed`로 새 작업을 즉시 닫고 비율 창을 다시 시작하며, 이후에는 일반적인 30초 단계 복귀가 필요하다. 일관되지 않은 읽기에는 알 수 없는 수준, 누락된 볼륨, 되돌아간 counter나 시계가 포함된다. Stale·미래·replay·다른 boot의 sample은 거절한다. Sampling이 멈추면 마지막 sample이 6초보다 오래된 시점에 admission이 닫힌다.
- **활성화.** `serve`는 배타적으로 보유한 journal을 native 시계·backend로 활성화하므로 boot 인지 복구에 실제 정체성을 사용한다. Backend는 macOS plan을 반환한다. 이는 QoS·우선순위를 통한 협조적 CPU, 회계 대상 메모리·task, 관측 process group이다. Scope binding, scope 관측, launcher 증거는 제공하지 않으므로 어떤 attempt도 이를 통해 bind되거나 회수되지 않는다. 인증된 status의 `registration_ready`와 `execution_ready`는 false를 유지한다. 서비스는 활성화, baseline, 상태 전이, 거절된 sample, 늦은 관제 loop 기상, 실패, 1분 heartbeat를 stderr에 JSON-line receipt로 남기며 자격 정보는 포함하지 않는다. Sampler가 어떤 이유로든 멈추면 서비스도 오류와 함께 멈춘다. 종료를 시작한 뒤 3초 안에 돌아오지 않는 probe는 종료를 막지 못하도록 버리며, 이때 서비스는 오류와 함께 종료한다.

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

- 남은 DG-1: wire 등록, 자원 정책 적용과 scope 증거, helper, 실행 CLI, Cargo, bounded 자기 적용, update·repair, 측정한 macOS SLO.
- CS-RG: Runner 슬롯·전송 lane, 승인 migration, client pin, 상태 결합과 회귀 qualification.
- DG-LINUX: 실제 cgroup 계층·controller·ancestor와 sandbox·proxy 포함.
- DG-CACHE·DG-ADAPTERS: 등록 cache 회수와 추가 도구 제어.

Journal·라이브러리는 사용자 프로세스 handle·출력·CodeSpace workspace lease·approval row를 소유하지 않는다. 자원 예약 수명으로 이들의 수명을 추정하지 않는다.
