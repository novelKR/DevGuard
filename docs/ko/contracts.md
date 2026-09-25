# 구현된 authority 계약

이 문서는 구현된 DG-0 authority, C01/C02 서비스·저장소·transport 동작, C03 native macOS 호스트 증거, C04 협조적 정책·scope 증거, C05 fenced launch helper, C06 대조, C07 명령행 owner, C08 Cargo adapter, 현재 사용자 LaunchAgent로의 C09 설치, C10 부모 lease와 후보 authority, C11 upgrade와 repair, C12 SLO 판정 harness를 설명한다. PR과 병합 후 main 전달 증거는 구현과 별도로 추적한다. 실제 제공 명령은 [운영 문서](operations.md)에 있다. Native 호스트 증거가 있으면 서비스는 등록·launch·대조를 열며, CodeSpace 결합은 아직 구현하지 않았다. [영문 정본](../contracts.md).

## Authority와 transport 경계

Core는 Rust 라이브러리다. Authority::register는 TrustedPeer를 받고, 설정 UID·소비자 자격·generation·정확한 프로세스 정체성을 Backend로 검증한다. 성공하면 불투명 Principal을 반환하며 workload는 공개 API로 control-service Principal을 만들 수 없다. Workload 소비자는 제어 예약을 설정할 수 없다. 관리 대조와 generation 폐기는 신뢰하는 daemon·운영자 경로의 책임이며 workload RPC로 공개하면 안 된다.

DG-0는 가짜 peer·backend로 라이브러리 등록 경계를 검증한다. C02는 실제 로컬 UDS 인증과 작은 client를 제공하지만 native 등록은 활성화하지 않는다. 서버는 OS socket 자격으로 peer UID/PID를 관측하고, client는 handshake의 authority UID/PID와 자기 정체성을 독립적으로 대조한다. 호출자가 선언한 peer 정체성을 신뢰하지 않는다. C03은 native boot/start 정체성을 추가하므로, OS가 관측한 peer UID/PID와 kernel start 정체성을 결합하면 완전한 신뢰 등록 관측이 된다. Native 시험은 이 경로를 프로세스 안에서 검증한다. Native 호스트 증거가 있으면 서비스는 wire 등록과 아래의 fenced launch helper를 대조와 함께 연다. 다른 플랫폼이거나 macOS 관측이 실패해 그 증거가 없으면 workload/control-service 등록은 ResourceControlUnavailable을 반환한다. 인증만으로 Principal·instance 슬롯·lease·호스트 예산을 발급하지 않는다. 관리 자격으로는 등록할 수 없다.

Handshake는 contract 호환성과 wire version 1을 확인한다. Native 증거가 있는 서비스는 내구성 admission, fenced launch, 자원별 증거, 정적 제어 예약, macOS 협조적 제어를 광고하며, 증거가 없으면 아무것도 광고하지 않는다. 부모 lease(C10)는 이를 요구한 client에게만 알린다. Consumer generation과 자격 digest가 workload/control-service 역할을 결정하며 별도 관리 digest는 명시적으로 공개한 관리 역할에만 사용한다. 모든 consumer·관리 digest는 서로 달라야 한다. Caller 자격, 일회성 helper permit과 관리 작업은 별도 경계이며 helper 자격 variant를 caller 인증으로 받지 않는다. 이는 협조적인 운영 계정 모델이며 악의적인 동일 UID 프로세스를 격리하지 않는다.

Authority는 journal 부모 디렉터리에 no-follow 배타 잠금을 보유한다. 같은 디렉터리의 모든 journal은 그 잠금을 공유한다. 잠금은 O_NONBLOCK으로 열고 열린 descriptor가 현재 UID 소유의 private 일반 파일이며 링크가 정확히 하나인지 명시 초기화 때도 확인한다. FIFO·링크된 파일은 잠금 대신 사용할 수 없다. C01은 caller HOME·XDG가 아니라 OS 계정으로 정상 경로를 결정하고 state·socket override를 거절한다. 프로젝트 설정에는 authority 자격이나 호스트 용량을 둘 수 없다. 후보 경로는 부모 lease 아래에만 있다(C10). `devguardd candidate`는 이를 계정 자신의 authority에서 정하며 다른 경로는 받지 않는다.

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
- **활성화.** `serve`는 배타적으로 보유한 journal을 native 시계·backend로 활성화하므로 boot 인지 복구에 실제 정체성을 사용한다. Backend는 macOS plan을 반환한다. 이는 QoS·우선순위를 통한 협조적 CPU, 회계 대상 메모리·task, 관측 process group이다. 다음 절의 scope 증거와, 대조 절에서 설명하는 claim되지 않은 grant에 helper가 없다는 owner 보고 증거도 제공한다. 인증된 status의 `registration_ready`와 `execution_ready`는 true다. 서비스는 활성화, baseline, 상태 전이, 거절된 sample, 늦은 관제 loop 기상, 실패, 1분 heartbeat를 stderr에 JSON-line receipt로 남기며 자격 정보는 포함하지 않는다. Sampler가 어떤 이유로든 멈추면 서비스도 오류와 함께 멈춘다. 종료를 시작한 뒤 3초 안에 돌아오지 않는 probe는 종료를 막지 못하도록 버리며, 이때 서비스는 오류와 함께 종료한다.

## Native 정책 적용과 scope 증거

macOS에서 scope는 root가 이끄는 process group이다. Root는 launch helper이며, 이후 helper가 executable로 바뀐다. Workload는 협조적이고 그 group 안에 머문다고 가정한다. Backend는 임의 자손의 격리나 kernel 메모리·task 제한을 주장하지 않으며 `NOTE_TRACK`을 사용하지 않는다.

- **Root 준비.** Root는 스스로 process group leader가 되고 utility QoS class(`POSIX_SPAWN_SETEXEC`와 `posix_spawnattr_set_qos_class_np`)로 자신을 다시 실행한다. 이때 PID, group, 환경, 상속 descriptor는 유지된다(`become_scope_root`, `exec_with_workload_qos`). 자손은 QoS clamp와 nice 값을 상속한다.
- **Scope 수립.** `NativeBackend::establish_scope`는 같은 사용자의 같은 boot에서 살아 있고, 자기 group을 이끌며, 아직 그 group에 혼자 있는 root만 받는다. 그다음 authority가 root의 nice를 +10으로 올리고(더 높은 값은 낮추지 않음) 결과를 다시 읽는다.
  - CPU는 `pbi_nice`가 10 이상이고 task 우선순위(`pti_priority`)가 20 이하이며 읽을 수 있었던 모든 thread의 최대 우선순위(`pth_maxpriority`)가 20 이하일 때만 적용으로 본다. 20은 utility 상한이다. Clamp가 없으면 task 우선순위는 31에서 nice를 뺀 값, thread는 63으로 읽히므로 clamp 누락을 드러내는 것은 thread 검사다.
  - Root의 정체성은 nice를 적용하기 직전과 우선순위를 읽은 뒤에 다시 확인하므로 재사용된 PID는 증거를 만들지 못한다. Authority 프로세스 자신은 scope root가 될 수 없다.
  - 메모리·task는 authority의 회계로 적용한다. Kernel 방식은 미지원이다.
  - 적용에 실패한 scope도 종료할 수 있도록 계속 추적한다. `AppliedResources::confirms`가 모든 자원의 적용을 요구하므로 binding은 이를 거절한다.
- **Binding.** `binding`은 수립 결과를 믿지 않고 등록된 scope의 readback을 다시 수행한다. 알 수 없거나 변경된 scope에는 증거가 없다. Scope ID는 root의 PID와 start 정체성으로 만든다.
- **관측.** Group ID는 그것이 여전히 이 scope의 group을 가리킨다고 입증될 때만 신뢰한다. 따라서 관측은 group을 나열(`PROC_PGRP_ONLY`)하기 전과 후에 root를 확인한다. Root가 살아 있거나 reap되지 않은 채로 PID를 보유하는 동안에는 다른 group이 그 ID를 쓸 수 없다. 그다음 나열된 모든 구성원의 정체성을 다시 확인한다.
  - 알 수 없는 구성원은 root가 나열 전후 모두 PID를 보유했거나 같은 관측에서 알려진 구성원이 group에 보일 때만 편입한다. 그렇지 않으면 그 ID가 무관한 group을 가리킬 수 있으므로 추적 상실로 기록한다.
  - 나열되었지만 이미 다른 group으로 옮겨 간 알 수 없는 프로세스도 추적 상실이다. 재사용된 PID와 구분할 수 없기 때문이다.
  - 그다음 알려진 모든 정체성과, 살아 있는 알려진 구성원의 자식(`PROC_PPID_ONLY`)을 확인한다. 자식은 나열 뒤에도 부모의 정체성이 그대로일 때만 인정한다. Group 밖에 있는 알려진 정체성이나 자식은 이탈이다. 부모 관계를 확인할 수 없는 group 밖의 자식은 추적 상실이다.
  - Reap되지 않은 zombie는 여전히 존재로 본다. Root는 start 정체성이 사라졌을 때만 reap된 것으로 본다.
  - 빈 group은 두 번째 나열로 확인하며 그 나열 자체가 비어 있어야 한다. 거기서 나열된 구성원은 읽기 전에 사라져도 존재로 세며, 거기서 나타난 구성원도 같은 편입 규칙을 따른다.
  - 사라진 것으로 확인된 정체성은 정리한다. (PID, start) 쌍은 다시 나타날 수 없기 때문이다. Root가 reap되고 group이 비었음이 확인되면 그 ID는 다시 나열하지 않는다.
  - 거부되거나 실패한 읽기는 추적 상실로 기록한다. errno를 설정한 채 항목 0개를 반환한 libproc 나열도 여기에 포함된다.

  이탈과 추적 상실은 모두 scope에 고정되며 이후의 평범한 관측으로 해제되지 않는다. 관측 시각은 관측이 끝날 때 기록한다.
- **종료.** `signal_scope`는 알려진 정체성과 알려진 이탈 정체성에 양수 signal을 PID 하나씩 보낸다. 현재 group 구성원에게도 보내지만, group이 여전히 이 scope의 것이라고 입증될 때만 보낸다. 각 대상은 signal 직전에 다시 확인한다. Root를 관측할 수 없거나 group을 입증할 수 없거나 나열·전달이 실패해도 확인된 정체성에는 signal을 보내며, 그 대신 receipt를 불완전으로 표시한다. 오래된 PID나 process group에는 보내지 않으며 전달이 회수 증거가 되지는 않는다. macOS에는 프로세스 handle이 없으므로 재확인과 signal 사이에 PID가 재사용될 가능성은 남는다.
- **회수.** 회수에는 여전히 core의 완전한 증거가 필요하다. 자손이 남은 채 reap된 root, zombie 구성원, 알려진 이탈, 불완전한 추적은 각각 회수를 막는다. bind되지 않은 committed launch는 helper가 없다는 owner의 보고가 있을 때만 회수한다([대조](#대조) 참고).
- **한계.** Authority 프로세스는 scope 추적을 메모리에 보유한다. 재시작 후에는 이전에 bind된 scope를 backend가 알지 못하므로 해당 attempt는 reboot가 종료를 입증할 때까지 추적 상실과 함께 Suspect로 남는다. setuid 프로그램처럼 authority가 관측할 수 없는 구성원은 해당 scope의 추적 상실을 영구히 만든다. Group에 알려진 구성원이 남지 않은 상태에서 root가 reap되면, 살아남은 알 수 없는 구성원이 이 scope의 것임을 입증할 수 없다. 이들은 영구 추적 상실이 되고 signal도 받지 않으므로 root가 reap되기 전에 scope를 관측해야 한다. Owner는 종료한 root를 아직 reap하지 않은 상태에서 `Observe`로 이를 수행한다.

## Fenced launch helper

DG1-C05는 launch helper(`devguard-launch`)와 이를 위한 wire 요청을 추가한다. 서비스는 native 호스트 증거가 있을 때만 이를 열며, 아래의 대조와 함께 연다.

- **등록.** 인증을 마친 소비자 session은 자기 instance를 등록한다. Authority는 OS가 관측한 peer와 kernel start 정체성에서 프로세스 정체성을 얻으며, 호출자는 instance ID만 제공한다. Session은 제한된 교환이므로 새 session마다 다시 등록하며, 이는 같은 instance를 다시 활성화한다.
- **Admission과 grant.** `Admit`은 attempt를 기록하고 `BeginLaunch`는 이를 commit한다. 일회성 permit은 첫 `BeginLaunch` 응답에만 들어 있다. Attempt는 등록된 instance에 속하며, 다른 instance는 이를 조회·취소·commit할 수 없다.
- **Helper 시작.** Owner는 grant마다 helper를 최대 하나, 자신의 직접 자식으로 시작한다(`devguard_client::launch::helper_command`). Permit은 private descriptor로 전달한다. 두 번째 private descriptor는 executable 자체 출력과 분리된 helper transcript를 전달한다. 인자와 환경에는 비밀이 없다. 그다음 helper는 다음 순서로 진행한다.
  1. 자기 process group을 이끈다. Pseudo-terminal의 session leader는 이미 그렇다.
  2. Utility QoS clamp 아래에서 자신을 다시 실행하며 PID와 두 descriptor를 유지한다.
  3. Permit descriptor를 소비하고 닫으며, transcript를 close-on-exec로 표시한다.
  4. 다른 요청을 할 수 없는 자기 session에서 grant를 제시한다.
  5. 첫 번째 authorization이 성공하면 READY를 보고하고 program을 실행한다. Executable은 helper의 PID와 process group, 작업 디렉터리, 환경과 그 밖의 모든 상속 descriptor를 유지한다.
- **Authority 검사.** 아래 검사는 모두 하나의 authority 잠금 안에서 수행하므로 취소·대조·다른 helper가 끼어들 수 없다.
  - Owner는 이번 서비스 수명 안에 등록되어 있어야 한다.
  - 제시한 프로세스는 같은 사용자의 살아 있는 프로세스이고 부모가 owner여야 하며, owner도 아직 실행 중이어야 한다.
  - Permit은 이 grant의 것이어야 한다.
  - Grant는 아직 commit 상태이거나 바로 이 helper가 이미 claim한 상태여야 한다.

  그다음 authority는 helper의 scope를 수립하며 nice를 적용하고 정책을 다시 읽는다. 여기까지 처음 도달한 helper가 grant를 claim한다. Bind하기 전에 그 scope를 내구성 있게 기록하므로, 그 뒤에 거절된 helper도 scope로 정리되며, 수립에 실패한 helper는 grant를 소모하지 않는다. 그다음 authority는 scope를 bind하고 실행을 authorize한다. 두 번째 helper, 취소 뒤에 온 helper, 실행 뒤에 온 helper는 모두 거절된다. 응답이 유실된 뒤 같은 helper가 다시 제시하면 `may_exec = false`를 받으며 실행하지 않는다.
- **거절.** Claim 전에 거절된 helper는 grant를 claim하지 않으므로 owner 자신의 helper가 여전히 그 grant를 쓸 수 있다. Claim 뒤에 거절된 helper는 종료된다. 예를 들어 readback에 utility clamp가 없는 경우다. 그 scope는 계속 과금되며 종료가 관측될 때만 회수된다. 이 회수는 executable이 시작하지 않았다는 증거가 아니다.
- **Transcript.** 한 줄에 JSON 객체 하나를 쓴다.
  - `failed`: helper가 grant를 제시하지 못했으므로 claim하지 않았다.
  - `refused`: authorize되지 않았거나 응답이 유실되었다. Helper는 절대 실행하지 않는다.
  - `ready`: exec 전 경계를 모두 마쳤다. Executable이 시작했다는 증거는 아니다.
  - `exec_failed`: READY 뒤의 exec가 실패했다.

  Exec가 성공하면 이 descriptor가 닫힌다. Helper는 authorize되지 않으면 125, program이 없으면 127, 그 밖의 exec 실패에는 126으로 종료한다. 그 밖의 종료 상태는 executable의 것이다.
- **Descriptor.** Helper는 자기 descriptor인 permit 전달체, transcript, authority session만 닫는다. Owner가 상속 가능하게 남긴 나머지는 명령 의미의 일부로 executable에 전달되므로, owner는 관계없는 descriptor를 close-on-exec로 유지해야 한다. macOS는 pipe나 socket pair를 close-on-exec로 원자적으로 만들 수 없다. 따라서 client는 grant의 descriptor를 표준 세 개보다 높은 번호로 만들고 helper spawn과 함께 하나의 프로세스 전역 guard 아래에서 수행하며, `HelperCommand::spawn`은 owner 쪽 사본을 닫는다. 다른 thread에서 다른 프로세스를 spawn하는 owner는 같은 guard(`spawn_guard`)를 잡거나 기본적으로 close-on-exec로 spawn해야 한다.
- **Receipt.** 제시마다 `helper_authorized`, `helper_replayed`, `helper_refused` 중 하나의 receipt를 남기며, claim 전 거절도 포함한다. Authorization에 걸린 시간을 기록한다. Helper의 250 ms 기한을 넘긴 응답은 helper에게 거절이며 helper는 실행하지 않는다.
- **호환성.** Journal schema는 바뀌지 않는다. Claim했지만 bind하지 않은 attempt는 scope가 있고 적용 자원이 없는 commit record다. 이전 artifact도 이를 읽으며, 다른 commit launch와 같이 재시작 때 Suspect로 전환한다. Wire version 1에는 서비스가 fenced launch를 광고한 뒤에만 client가 사용하는 요청과 응답이 추가된다.

## 대조

DG1-C06은 서비스의 reconciler와 attempt를 정리하는 owner 요청을 추가한다. 시간 초과, 연결 끊김, owner 사망, reap된 root 중 어느 것도 단독으로는 회수 근거가 되지 않는다.

- **Reconciler.** 서비스는 매초 과금 중인 attempt를 모두 대조하며, authority 잠금은 attempt 하나마다 잡는다.
  - Prepared attempt는 원래의 5초 기한에 만료된다.
  - Claim되었거나 bind된 attempt는 scope가 끝났다고 관측될 때만 회수한다. Root가 reap되고, group이 비고, 알려진 구성원이 사라지고, 추적이 완전하며, 이탈이 없어야 한다. 이탈이나 불완전한 관측은 고정되는 추적 상실과 함께 Suspect로 만든다.
  - Claim되지 않은 grant는 helper가 아직 claim할 수 있으므로 owner가 실행 중인 동안 그대로 둔다. Owner가 사라지면 어떤 helper도 claim할 수 없으므로 Suspect가 되며, 회수하지는 않는다.
  - 이전 boot의 scope는 추적 상실 뒤에도 이전 boot 종료로 회수한다.
  - 대조로 바뀌지 않은 attempt는 다시 쓰지 않는다.

  실패한 대조 pass는 `reconcile_failed`로 보고하고 다시 시도한다. Authority 잠금이 오염된 경우를 포함해 reconciler가 멈추면 서비스도 멈추므로 launch가 대조 없이 열린 채로 남지 않는다.
- **Instance.** Session은 제한된 교환이므로 session을 닫는다고 instance가 suspect가 되지 않는다. 기준은 등록된 프로세스의 수명이다. 그 프로세스가 사라지면 과금 중인 attempt가 없을 때 instance를 폐기하고, 있으면 suspect로 둔다. 그 instance를 지정한 helper는 거절한다. 이전 boot의 프로세스는 지금 그 PID를 누가 쓰든 끝난 것이므로, reboot 뒤에는 다른 사용자의 프로세스가 옛 owner의 PID를 쓰더라도 옛 owner를 정리한다. 같은 boot 안에서 kernel이 관측을 거부하면 대조를 미룰 뿐이다. 등록할 때 해당 consumer의 instance pool이 가득 차 있으면, 서비스는 먼저 프로세스가 끝난 instance에 이 규칙을 적용한다. 따라서 이미 종료한 owner의 자리는 다음 pass를 기다리지 않고 바로 비워진다. 끝났지만 과금 중인 작업을 가진 owner는 그 작업이 정리될 때까지 자리를 유지한다.
- **Helper 미생성.** Grant의 permit은 owner만 가지므로 owner는 그 grant의 helper가 없고 앞으로도 시작하지 않는다고 보고할 수 있다(`AbandonLaunch`). Helper 생성이 실패했거나, helper가 READY 전에 종료되어 reap되었거나, permit을 담은 응답이 도착하지 않은 경우다. 이 보고에는 permit이 필요 없고, owner의 등록 정체성에 묶이며, 메모리에 보관한다. 보고가 있으면 claim되지 않은 grant를 `NoHelperCreated`로 회수하며, 이는 executable이 시작하지 않았다는 증거다. Claim된 grant에는 helper가 있으므로 보고로 회수할 수 없고 그 scope로만 정리된다. 이 회수가 안전한 이유는, 보고와 회수가 모든 claim과 같은 authority 잠금 안에서 이루어지고, 회수되었거나 Suspect인 grant는 절대 claim·bind·authorize될 수 없어 늦은 helper가 executable을 시작할 수 없기 때문이다. Journal도 scope가 있는 `NoHelperCreated` record와 scope가 없는 `ScopeTerminated` record를 거절한다.
- **Reap 전 관측.** `Observe`는 owner의 attempt를 같은 규칙으로 즉시 대조한다. 종료했지만 아직 reap되지 않은 root는 PID를 보유하므로 그 group에 남은 구성원을 편입할 수 있다. Owner가 먼저 reap하면 살아남은 구성원이 이 scope의 것임을 입증할 수 없으므로 추적 상실이 되고 attempt는 Suspect로 남는다.
- **종료.** `Terminate`는 claim되었거나 bind된 scope의 다시 확인한 정체성마다 interrupt·hangup·terminate·kill 중 하나를 보낸다. Signal을 보낸 정체성의 수와 모든 대상을 관측할 수 있었는지를 보고한다. 전달은 phase를 바꾸지 않으며 회수 증거가 아니다. 재시작 뒤에는 scope를 알 수 없으므로 서비스가 signal을 보낼 수 없다.
- **재시작.** 재시작은 commit된 모든 attempt와 등록된 모든 instance를 Suspect로 만들며, owner는 새 session에서 다시 등록한다. 이전에 bind된 scope는 프로세스가 끝나도 reboot 전까지 추적 상실과 함께 Suspect로 남는다. Claim되지 않은 grant는 실행 중인 owner의 보고로만 회수한다. 재시작 전후의 회계 합계는 같다.
- **Receipt.** 서비스는 자격 정보 없이 다음 JSON line을 stderr에 남긴다: `registered`, `admission`, `launch_committed`, `cancelled`, `helper_authorized`, `helper_refused`, `helper_replayed`, `launch_abandoned`, `scope_signalled`, `attempt_reconciled`, `instance_reconciled`, `reconcile_failed`, `reconcile_stopped`.

## 명령행 owner

DG1-C07은 관리 실행의 명령행 owner인 `devguard`를 추가한다. 명령은 authority와 fenced helper를 거쳐서만 실행한다. 명령을 admission하거나 시작할 수 없으면 아무것도 실행하지 않으며, 관리되지 않는 실행으로 대체하지 않는다.

- **Authority.** 바이너리는 운영 계정에서 authority를 결정한다. 정상 socket, 운영자 설정, `dev-cli` 자격 파일을 사용하며 경로·socket·authority override는 받지 않는다. `dev-cli`로 인증하고 CLI 프로세스마다 새 instance를 등록하므로, 설정된 instance 상한 8개가 동시 owner 수의 상한도 된다. 등록을 거절하기 전에 가득 찬 pool을 대조하므로, 이미 종료한 owner가 pool을 채우는 일은 없다. 실행 중인 owner로 가득 찬 pool은 용량 거절과 같이 처리한다. Frame마다 250 ms 기한이 있으므로 요청마다 새 session을 쓴다.
- **준비.** Admission 전에 CLI는 shell을 실행하지 않고 shell과 같은 방식으로 프로그램을 찾는다. Slash가 있는 이름은 경로이고, 이름만 있으면 `PATH`에서 찾는다. 프로그램이 없으면 127, 실행할 수 없으면 126으로 끝난다. Executable은 절대 경로를 argv[0]로 삼아 실행되며, CLI의 작업 디렉터리·환경·표준 descriptor와 adapter의 변경을 상속한다. UTF-8이 아닌 인자는 거절한다.
- **의미.** 실행 digest는 executable의 경로·device·inode·크기·수정 시각, 작업 디렉터리의 정체성, adapter 변환 뒤의 argv와 환경 변경, 표준 입력이 terminal인지 여부, 자원 의도를 포함한다. CLI는 자체 timeout을 두지 않으며 이를 최댓값으로 기록한다.
- **프로젝트와 예산.** `--project ID`는 운영자가 등록한 프로젝트여야 하며, 그 root가 작업 디렉터리를 포함하고 `.devguard.toml`이 같은 프로젝트를 가리켜야 한다. 프로젝트의 `.devguard.toml`은 symbolic link를 따라가지 않고 막히지 않게 열며, 일반 파일이어야 한다. 요청량은 필드마다 명시한 `--cpu`·`--memory`·`--tasks`가 우선하고, 다음은 프로젝트 상한, 그다음은 adapter 기본값이다. Generic 기본값은 1,000 mCPU·1 GiB·32 task이며 검증되지 않은 초기값이다. 프로젝트 상한을 넘는 요청은 거절한다. Admission을 강제하지 않는다.
- **대기.** `--wait`가 없으면 거절로 실행을 끝낸다. `--wait DURATION`(최대 24시간)이 있으면 용량이나 압력 때문인 거절, 또는 가득 찬 instance pool을 새 attempt로 다시 시도한다. 재시도 간격은 250 ms에서 시작해 두 배씩 늘어 10초가 상한이며, 기한까지 계속한다. 거절된 attempt는 모두 시작하지 않았음이 알려진 종결 record이며, 그 generation이 폐기될 때까지 journal에 tombstone으로 남는다. 재시도 간격 상한 덕분에 긴 대기도 이런 record를 10초에 하나 넘게 만들지 않는다. 대기 전에 CLI는 요청을 호스트의 작업 용량과 비교한다. 작업 용량은 서비스와 같은 방식으로 운영자 설정과 관측한 호스트에서 구한다. 결코 들어갈 수 없는 요청은 attempt 없이 즉시 거절한다. 그 용량을 관측할 수 없으면 receipt에 그렇게 기록하고, 판단은 평소처럼 authority가 한다. Signal은 대기를 취소하고 CLI를 그 signal로 끝낸다. 그 밖의 거절은 다시 시도하지 않는다.
- **유실된 응답.** 유실된 admission 응답은 같은 key로 한 번 다시 보낸다. 유실된 launch commit 응답은 조회한다. Commit된 grant는 받지 못했다고 보고하며(`AbandonLaunch`), `NoHelperCreated`로 회수되면 대기 시간 안에서 새 attempt를 만들 수 있다. 확인하거나 회수할 수 없는 grant는 보고만 하고 다시 시도하지 않는다. 새 attempt는 시작하지 않았음이 알려진 뒤에만 만든다.
- **Launch.** CLI는 helper를 직접 자식으로 시작하며, helper는 처음부터 자기 process group을 이끈다. 표준 descriptor 중 하나가 CLI의 제어 terminal이고 CLI가 foreground를 가지고 있으면, CLI는 terminal을 workload의 group에 넘겨 terminal 키가 workload에 바로 전달되게 한다. 입력이 redirect되어도 출력이 terminal에 닿으면 마찬가지다. Workload가 멈추면 terminal을 되찾고, 종료한 뒤에는 root를 reap하기 전에 되찾는다. READY 전에 거절된 helper는 reap하고, claim되지 않은 grant를 `NoHelperCreated`로 회수한다. 일시적인 거절은 대기 시간 안에서 다시 시도할 수 있다. 다만 launch 중에 CLI가 signal을 받았으면 실행을 취소한다.
- **Launch transcript.** CLI는 root를 기다리는 동안 별도 thread에서 helper의 transcript를 끝까지 읽는다. 그래서 READY 전의 정지도 즉시 반영하며, 기한을 두지 않는다. Helper 쪽 끝은 executable이 시작되거나 helper가 종료하면 닫힌다. READY 없이 끝난 transcript는 executable이 실행되지 않았다는 뜻이다. Transcript를 올바른 끝까지 읽을 수 없거나, READY 뒤에 마지막 보고가 없거나, root를 더 기다릴 수 없으면 결과는 `uncertain`이다. CLI는 명령이 시작되었는지 알 수 없다고 보고하고, root 자신의 종료값으로 끝나며, 다시 시도하지 않는다. 그래도 helper를 가지고 있지 않다고 보고하므로 claim되지 않은 grant는 정리될 수 있다. Claim된 grant는 그 scope로 정리된다.
- **Signal.** CLI가 받은 SIGINT·SIGTERM·SIGHUP·SIGQUIT는 workload의 process group에 전달한다. Root가 reap되지 않은 동안에만 전달하므로 group ID가 다른 group을 가리킬 수 없다. `nohup`에서처럼 CLI가 무시 상태로 상속한 signal은 계속 무시하며 전달하지 않는다. Workload도 같은 설정을 상속한다. Workload의 job control 정지(SIGTSTP·SIGTTIN·SIGTTOU)는 그대로 반영한다. CLI가 terminal을 되찾고 같은 signal로 스스로 멈추므로 CLI를 시작한 shell이 제어를 되찾는다. 재개되면 terminal을 다시 넘기고 workload를 재개한다. SIGSTOP이나 tracer로 인한 정지처럼 그 밖의 정지는 그 원인 제공자에게 맡기고, CLI는 계속 기다린다.
- **Reap 전 관측.** CLI는 root의 종료를 reap하지 않고 기다렸다가, 종료한 root가 아직 PID를 보유한 동안 authority에 attempt의 `Observe`를 요청한다. 그다음 reap하고 다시 관측한다. 따라서 group에 남은 구성원도 추적되며, 끝날 때까지 attempt의 과금을 유지한다.
- **종료값.** CLI는 workload의 종료값으로 끝나거나 같은 signal로 끝난다. Signal로 스스로 끝나기 전에 자신의 core file 상한을 0으로 설정하므로 core dump는 workload만 남길 수 있다. 아무것도 시작하지 않았으면 `env`·`timeout`처럼 125로 끝나며, helper의 126·127은 그대로 전달한다.
- **Receipt.** `--receipt PATH`는 admission 전에 새 private 파일을 만들고 `devguard-exec-receipt/v1`을 기록한다. 결과(`completed`·`exec_failed`·`not_started`·`uncertain`), 명령, adapter 보고, 예산과 그 출처, 프로젝트, authority, 예약과 lease ID를 포함한 모든 attempt, 비교에 쓴 작업 용량을 포함한 대기, helper phase, 종료값, reap 전후의 관측, 받은 signal·전달한 signal·무시 상태로 상속한 signal을 담는다. Adapter가 설정하거나 제거한 변수는 이름만 기록하고 상속된 값은 기록하지 않으며, permit과 호출자 자격은 담지 않는다.
- **Doctor.** `devguard doctor`는 경로, 설정, helper, 서비스의 정체성·capability·상태를 보고하고, 선택적으로 등록과 프로젝트도 확인한다. 관리 실행을 사용할 수 있는지와 관리되지 않는 대체 실행이 없다는 점을 밝힌다. `--require admission,registration,macos-cooperative`를 주면 요구 사항이 하나라도 충족되지 않을 때 1로 끝난다.
- **중첩 실행.** Workload가 `devguard exec`를 다시 실행하면 자체 예약과 scope를 가진 별도의 관리 실행이 시작된다. 이 실행은 바깥 scope에 과금되지 않고 바깥 scope에 포함되지도 않는다. 중첩 작업을 부모 예산 안으로 제한하려면 명시적인 부모 lease(`--lease`, C10)가 필요하다.
- **Adapter.** C07은 admission과 분리된 adapter 인터페이스를 정의한다. Adapter는 실행될 예약에 맞춰 인자와 환경을 바꿀 수 있으며 무엇을 했는지 보고한다. Generic adapter는 아무것도 바꾸지 않는다. Cargo adapter는 아래에서 설명한다.

## Cargo adapter

DG1-C08은 command가 실행되는 예약에 맞춰 Cargo의 compiler 병렬도를 조정하는 Cargo adapter(`devguard-cargo`)를 추가한다. Cargo jobs는 compile만 제한하며, Cargo가 실행하는 시험 프로그램의 thread 수는 제한하지 않는다. 메모리는 회계 추정치이며 측정된 강제가 아니다.

- **추정.** Compiler job 하나에는 논리 CPU 1개와 1.5 GiB가 필요하고, build마다 고정 512 MiB가 더해진다. 따라서 예약이 수용하는 job 수는 min(CPU ÷ 1,000 mCPU, (메모리 − 512 MiB) ÷ 1.5 GiB)이다. Job 하나도 수용할 수 없는 예약은 admission 전에 거절하며, job을 강제하지 않는다. Cargo 기본 요청은 job 2개(2,000 mCPU·3.5 GiB·64 task)이며 검증되지 않은 초기값이다.
- **선택.** `--adapter cargo`는 `cargo`만 실행하며 다른 프로그램은 거절한다. `--adapter cargo-pipeline`은 검증 script처럼 Cargo를 실행하는 프로그램을 위한 것이다. `auto`는 프로그램이 compile하는 subcommand(build·check·test·bench·run·doc·clippy·rustc·rustdoc·install·fix와 그 별칭)를 쓰는 `cargo`일 때 Cargo adapter를, 그 밖에는 generic adapter를 고르고 이유를 기록한다.
- **Direct mode.** Adapter는 Cargo 인자를 `--`까지 읽는다. `-j N`·`-jN`·`-j=N`·`--jobs N`·`--jobs=N` 형태의 명시적 `-j`/`--jobs`는 먼저 Cargo가 읽는 방식대로 해석한다. `default`는 호스트의 논리 CPU 수이고, 음수는 그 수에서 빼되 1보다 작아지지 않는다. 반복되거나 0이거나 해석할 수 없는 값은 jobserver 유무와 관계없이 거절한다. 예약 안의 값은 유지하고, 더 큰 값은 그 자리에서 예약의 job 수로 바꾼다. 값이 없고 적용되는 jobserver도 없으면 subcommand 뒤에 `--jobs N`을 넣으며, 이는 `CARGO_BUILD_JOBS`와 설정보다 우선한다. `CARGO_TARGET_DIR`·보고서 경로·명령 선택은 바꾸지 않으며, receipt에 원래 job 수·적용한 job 수·이유를 기록한다. `--adapter cargo`에서 지원하지 않는 subcommand는 거절한다.
- **Pipeline mode.** Adapter는 새 0700 디렉터리에 N−1개 token을 가진 private FIFO jobserver를 만들며, job은 최대 1,024개다. CLI는 실행 동안 이를 열어 두었다가 끝나면 제거한다. 이 jobserver는 `CARGO_MAKEFLAGS`로 내보낸다. 프로그램이 직접 또는 중첩으로 시작하는 모든 Cargo가 이 pool을 공유하며, jobserver가 유효한 동안에는 자체 `-j`를 무시한다. Python `subprocess`처럼 상속 descriptor를 닫는 프로그램을 거쳐도 유지되도록 FIFO를 쓴다. 동시에 시작한 최상위 Cargo k개는 각자 암묵 job 하나를 더하므로 함께 최대 N−1+k개 job을 실행한다. 실행 뒤 receipt는 pool로 돌아온 token 수를 기록한다.
- **Fallback.** 두 mode 모두 `CARGO_BUILD_JOBS`를 예약의 job 수 N으로 설정하며, 호출자가 설정한 값은 대체한다. Cargo는 jobserver를 열지 못했고 명령행이나 `--config`에도 job 수가 없을 때만 이 값을 쓴다. 따라서 pool을 열지 못한 Cargo를 제한하면서도, 유효한 jobserver 아래에서 명시적 `-j`가 일으키는 경고는 생기지 않는다. Receipt는 이를 설정한 변수로 기록한다.
- **상속된 jobserver.** CLI가 `CARGO_MAKEFLAGS`·`MAKEFLAGS`·`MFLAGS`로 상속한 jobserver는 Cargo가 읽는 방식대로 검사한다. 처음 존재하는 변수가 열려 있고 상속 가능한 pipe 쌍이나 사용자 소유 FIFO를 가리켜야 한다. 유효한 jobserver는 승인 설계대로 두 mode 모두에서 보존되어 병렬도를 제한하며, receipt에는 그 크기를 관측할 수 없다고 기록한다. 두 번째 pool은 만들지 않는다. 이때 direct mode는 job 수를 넣지 않는다. Cargo가 무시한다는 경고만 낼 것이기 때문이다. 다만 명시적 값은 여전히 조정한다. Descriptor 쌍 jobserver는 상속 descriptor를 열어 두는 프로그램에만 전달된다. Pipeline mode에서 Python `subprocess`의 기본 동작처럼 descriptor를 닫는 프로그램이 시작한 Cargo는 `CARGO_BUILD_JOBS`로 돌아가며, receipt에 그렇게 기록한다. 닫힌 descriptor처럼 오래된 참조는 함께 온 job 수와 함께 환경에서 제거하므로, Cargo가 조용히 자체 pool로 돌아가지 않는다.
- **중첩 Cargo.** Cargo는 build script에 jobserver를 넘기므로, 중첩 Cargo는 새 pool을 만들지 않고 바깥 pool을 공유한다.

## 설치와 현재 사용자 서비스

DG1-C09는 패키지로 만든 release를 현재 사용자의 LaunchAgent로 설치한다. 서비스는 여전히 같은 foreground `devguardd serve`를 실행한다. 권한 있는 daemon이 아니며, worktree `target`의 바이너리를 설치된 서비스로 쓰지 않는다.

- **패키지.** `scripts/package.py`는 깨끗한 tree와 Rust 1.95.0을 요구한다. 한 번의 release build로 `devguardd`·`devguard`·`devguard-launch`를 만들며, C10 전까지 이 build는 bootstrap으로 표시한다. 그리고 `devguard-release-manifest/v1`에 다음을 기록한다.
  - release id: `<version>-<commit7>-<artifact digest>`;
  - source commit·tree·qualification tree digest;
  - build 명령과 toolchain;
  - 각 바이너리의 SHA-256과 크기;
  - build에 컴파일된 호환성. `devguardd version --json`이 이를 출력하며, 패키지 버전·wire 버전·protocol·capability·journal schema·설정 schema를 담는다.

  패키지는 기능 artifact(`scope: functional`, `slo_qualified: false`)이며, release를 qualification하는 것은 C12뿐이다.
- **검증.**
  - 패키지나 설치된 release에는 정확히 `MANIFEST.json`과 세 바이너리가 든 `bin/`만 있어야 한다. 바이너리는 symlink가 아닌 실행 가능한 일반 파일이어야 하며, 크기와 hash가 manifest와 일치해야 한다.
  - Manifest의 호환성은 installer 자신의 build와 같아야 한다.
  - Installer는 그 패키지에서 실행해야 한다. 즉 installer 자신의 실행 파일 hash가 manifest의 `devguardd`와 같아야 한다. 따라서 서로 다른 build의 바이너리가 섞이지 않는다.
- **설치.** `devguardd install --package DIR`는 다음 경우 거절한다.
  - authority가 정상 endpoint를 제공하거나 잠금을 보유한 동안;
  - agent가 load되어 있거나 plist가 있을 때;
  - authority 상태가 없을 때. Journal을 만들거나 고치거나 다시 쓰지 않는다.

  Release를 private 형제 디렉터리로 복사해 sync하고 읽기 전용으로 만든다(디렉터리와 바이너리 0500, manifest 0400). 그다음에야 `releases/<id>`로 rename하므로, 부분 복사본은 최종 이름을 갖지 않는다. 이미 있는 release는 manifest가 byte 단위로 같을 때만 재사용하며, 다른 release로 덮어쓰지 않는다. 끝으로 `~/Library/LaunchAgents/io.github.novelkr.devguard.plist`(0644)를 쓰고 사용자의 `gui/<uid>` domain에 bootstrap한다.
- **Agent.**
  - 프로그램은 release의 `devguardd serve`이며, `RunAtLoad`로 load와 login 때 시작한다.
  - `KeepAlive {Crashed: true}`이므로 crash 뒤에만 10초 throttle 후 다시 시작한다.
  - 정상 종료에서는 멈춘 채로 둔다. launchd의 SIGTERM, 그리고 journal이 없거나 손상되었거나 두 번째 authority가 잠금에 거절된 경우처럼 닫힌 채 끝나는 종료가 여기에 포함된다.
  - 출력은 `~/Library/Logs/DevGuard/devguardd.log`에 남으며, rotation은 수동이다.
- **선택 전 검증.** Installer는 15초 안에 다음을 관측해야 한다.
  - launchd가 보고하는 서비스의 PID가 endpoint handshake의 PID와 같을 것;
  - 그 PID의 실행 image(`proc_pidpath`)가 release의 `devguardd`이고 manifest의 hash와 같을 것.

  그다음에야 `releases/selection.json`(0600: current와 last known good, 이력)을 기록하고 `recovery/<id>`에 변경 불가능한 복구 사본을 보존한다. 검증에 실패하거나 복구 사본이나 선택을 기록할 수 없으면 job을 bootout하고 plist를 제거하며, 아무것도 선택하지 않는다.
- **상태.** `devguardd status`는 아무것도 바꾸지 않고 다음을 보고한다.
  - 선택과 launchd 상태;
  - plist가 이 build가 현재 release에 대해 만드는 plist와 정확히 같은지;
  - 실행 중인 PID·실행 파일·hash와 manifest의 비교;
  - release와 복구 사본.

  서비스가 검증된 current release나 그 복구 사본을 admission이 열린 채 실행할 때만 0으로 끝난다. 닫기를 따르는 release에서는 admission이 닫혀 있음을 보고한다.
- **재시작.** 다시 시작한 서비스는 다른 시작과 마찬가지로 기존 journal을 다시 열어 대조한다. Journal이 없거나 손상되었으면 닫힌 채 멈춰 있다.
- **범위.** 설치된 release를 교체하는 것은 upgrade(C11, 아래)이며 설치는 이를 거절한다. 제거 명령은 없다. `launchctl bootout gui/<uid>/io.github.novelkr.devguard`를 실행하고 plist를 지우면 서비스가 멈추며, 모든 release·복구 사본·선택·journal은 보존된다.

## 부모 lease와 후보 authority

DG1-C10은 안정 authority가 호스트 예산을 두 번 발행하지 않고, 제한된 예산 하나인 부모 lease를 후보 build 검증에 빌려주게 한다.

- **Capability.** 서비스는 handshake에서 `parent_lease`를 요구한 client에게만 이를 알린다. 따라서 이전 client는 decode할 수 없는 capability를 받지 않는다. Client는 이 capability를 확인한 뒤에만 lease 요청을 보낸다.
- **Lease.** 등록된 workload owner가 `AdmitLease {key, budget, ttl}`를 요청한다.
  - Lease는 다른 요청과 마찬가지로 현재 압력에서의 호스트 용량에 대해 admission된다. 전체 예산은 해제될 때까지 과금된다.
  - 첫 응답에만 일회성 64자 token이 들어 있다. 재요청은 token 없이 lease만 돌려준다. Journal은 token의 digest만 보관하며 receipt에는 token이 들어가지 않는다.
- **자식.** Lease와 같은 consumer·generation의 등록된 instance가 `AdmitChild {lease, token, request}`를 보내면, lease의 남은 예산에 대해 일반 attempt를 admission한다. 남은 예산은 lease 예산에서 과금 중인 자식들의 예약을 뺀 값이다.
  - 호스트 용량과 압력은 다시 확인하지 않으므로 살아 있는 lease가 줄어들지 않는다.
  - Token이 틀리거나 lease를 모르면 `Unauthorized`이다. 남은 예산보다 큰 자식은 `ResourceUnavailable`로 거절하고, 더 이상 active가 아닌 lease는 모든 자식을 `InvalidTransition`으로 거절한다.
  - Admission된 자식은 안정 `devguard-launch`를 거쳐 다른 attempt와 똑같이 launch·대조·해제된다. 해제된 예산은 호스트가 아니라 lease로 돌아간다.
- **종료.** Lease는 Active, Ending, Released 순으로 진행한다.
  - Owner가 끝내거나(`EndLease`), owner 프로세스가 끝났거나 이전 boot의 것이거나, 기한이 지나면 Ending이 된다. Ending lease는 자식을 admission하지 않는다.
  - 과금 중인 자식이 하나도 없으면 Released가 되고 예산은 호스트로 돌아간다. Suspect 자식은 lease를 과금 상태로 유지한다. Reconciler는 매 pass마다 lease를 정리한다.
- **보유자.** `LeaseStatus {key, token}`은 별도 session이다. Token만 제시하고 caller로 인증되지 않으며 다른 요청을 할 수 없다. Lease의 phase·예산·남은 예산·기한·자식을 보고한다.
- **Journal.** `leases`와 `lease_children` table은 없을 때 추가하며 journal schema는 1로 유지한다. 과금 용량은 해제되지 않은 모든 lease의 예산과, lease 자식이 아닌 과금 attempt의 예약을 더한 값이다. 해제되지 않은 lease를 가진 generation은 폐기할 수 없다. 모든 자식 연결이 journal에 있는 lease를 가리키고, 과금 중인 자식이 해제된 lease에 속하지 않을 때만 활성화하며, 그렇지 않으면 닫힌 채 실패한다. 이런 journal을 여는 C09 artifact는 새 table을 무시한다. 자식은 일반 attempt로 계산하고, lease의 쓰지 않은 남은 예산은 보유하지 않는다.
- **후보 authority.** `devguardd candidate --id ID --lease CONSUMER/GENERATION/ATTEMPT --capacity MILLICPU,BYTES,TASKS --token-fd N`은 전체 용량이 계정 authority의 부모 lease인 격리 authority를 제공한다.
  - 상속한 descriptor N에서 lease token을 읽고 닫는다. Token이 없으면 아무것에도 연결하지 않고 아무것도 만들지 않는다.
  - 보유자로서 부모에게 lease 상태를 묻는다. Lease는 active이고 용량을 담을 수 있어야 한다. 후보 daemon 자신의 예약(250 mCPU, 128 MiB와 bootstrap system task)을 넘지 않는 용량은 아무것도 만들기 전에 거절한다.
  - 상태·설정·자격·socket·cache는 authority root와 runtime 디렉터리의 `candidates/<id>` 아래에 두며, 아직 없어야 한다. 모든 후보는 새 상태에서 시작한다. 정상 상태·자격·release·복구 사본은 열지 않는다.
  - 정책은 lease 용량을 쓰며 호스트 headroom은 없다. 호스트 용량은 쓰지 않는다. 호스트 압력은 다른 authority와 같이 sample한다.
  - 내구성 admission, 자원별 증거, 정적 제어 예약, macOS 협조적 제어를 광고하지만 fenced launch와 부모 lease는 광고하지 않는다. Launch·helper·lease 요청은 `ResourcePolicyUnsupported`로 거절한다. Status는 등록 준비, 실행 미준비를 보고하며, 이유에 후보와 그 lease를 적는다.
  - 매초 lease를 확인하고 lease가 더 이상 active가 아니면 즉시 닫는다. 부모가 token을 거절하거나 5초 동안 lease를 확인하지 못하면 닫힌 채 끝난다.
  - 부모는 후보를 lease 자식으로 admission할 때 그 용량을 과금하므로, 후보의 admission은 lease를 넘을 수 없다.
- **CLI의 lease 자식.** `devguard exec --lease CONSUMER/GENERATION/ATTEMPT --lease-token-fd N`은 명령을 그 lease의 자식으로 admission한다. Descriptor N에서 token을 먼저 읽고 닫으므로 workload는 이를 상속하지 않는다. 부모 lease capability를 요구하고, receipt에 lease를 기록하며, 대기는 호스트 용량 대신 lease 예산과 비교한다.
- **Test-candidate.** `devguard test-candidate --candidate DIR --report DIR [--cpu MILLICPU] [--memory SIZE] [--tasks N] [--ttl DURATION] [--wait DURATION]`는 안정 authority의 lease 하나 안에서 후보 tree를 검증한다.
  1. Lease를 admission한다. 기본값은 CPU 2개, 4 GiB, task 96개이며 기한은 2시간이다. `--wait`를 주면 용량이나 압력으로 거절되는 동안 다시 요청한다.
  2. 각 workload를 `devguard exec --lease`로 lease 자식으로 실행한다. 순서는 tree의 daemon·CLI·helper `cargo build`, 시험이 process group이나 session을 만들지 않고 호스트 용량을 관측하지 않는 crate(contract, core, client, Cargo adapter)의 `cargo test`, 그리고 CPU 1개·1 GiB·task 64개를 쓰는 tree 자신의 `devguardd candidate`이다.
  3. 바깥에서 후보 endpoint를 점검한다. Capability와 status, 용량 안의 admission, 거절되는 launch, 취소, 용량을 넘는 요청의 거절, 거절되는 중첩 lease를 확인한다.
  4. Lease를 끝내고 후보가 닫히고 lease가 해제될 때까지 기다린 뒤 후보 영역을 지운다. Lease, 각 자식의 receipt와 예약, 모든 점검을 담은 `report.json`을 쓰며, 모두 통과했을 때만 0으로 끝난다.

  실패하면 먼저 lease를 끝낸다. 실행 중인 자식은 끝까지 실행되고, 모두 정리되면 lease가 해제된다. 명령 자체가 kill되면 owner 규칙에 따라 lease가 끝난다.
- **범위.** 자체 process group이나 session을 만드는 workload는 lease 자식의 scope를 벗어나 협조적 macOS 모델에서 Suspect로 남는다. 따라서 native launch·CLI·terminal·scope suite는 lease 자식이 아니라 bootstrap과 CI qualification 실행으로 남는다. 후보는 admission만 검증하며 후보의 workload는 후보를 거쳐 실행되지 않는다. 설치된 부모 아래의 실제 자기 적용에는 부모 lease를 광고하는 release가 필요하다. 호스트 메모리 압력으로 서비스가 Critical인 동안에는 lease를 admission하지 않는다.

## Upgrade와 repair

DG1-C11은 과금 중인 작업을 잃거나 중복하지 않고, 후보의 admission에 의존하지 않으면서 설치된 서비스를 교체하고 복구한다.

- **Staging.** `devguardd stage --package DIR`은 설치와 같은 방식으로 패키지를 검증하고 변경 불가능한 `releases/<id>`로 복사한다. 패키지에서 실행해야 하며, 서비스·선택·journal은 건드리지 않는다.
- **Admission 닫기.** 관리자 session은 admission을 닫고(`CloseAdmission {reason}`), 다시 열고(`OpenAdmission`), 아직 과금 중인 것을 물을 수 있다(`Quiescence`). 서비스는 `upgrade_drain`을 요구한 client에게만 이를 알린다.
  - 닫기는 효력이 생기기 전에 private marker `state/admission.json`을 기록한다. 따라서 다시 시작한 서비스나 다음 release도 admission이 닫힌 채로 시작한다. 모든 Prepared attempt는 취소되며 시작되지 않았음이 알려진다.
  - Admission이 닫힌 동안 admission, launch commit, 부모 lease, lease 자식은 `ResourceUnavailable`로 거절되며, 대기 중인 CLI는 이를 다시 시도한다. 조회·취소·정지·owner 보고·대조는 계속된다. Status는 실행 미준비와 닫은 이유를 보고한다.
  - 다시 닫아도 처음의 닫기가 유지된다. 다시 열 때는 효력이 생기기 전에 marker를 지우고 그 삭제를 영속화한다.
  - 보고는 과금 중인 attempt와 lease의 수와 각각 최대 16개의 key를 담으므로 항상 frame 하나에 들어간다.
  - C11 이전 release는 이 요청을 decode할 수 없어 응답 없이 연결을 닫는다. 따라서 release가 admission을 닫을 수 있는지는 그 manifest가 알리는 내용으로 판단한다. Native 증거 없이 제공 중인 release는 capability를 알리지 않고 아무것도 admission하지 않으므로, 닫을 수 없는 release로 취급한다.
- **한 번에 하나의 작업.** 설치·staging·upgrade·repair·재개는 authority root의 private 작업 lock `operations.lock`을 보유하며, 다른 작업이 보유 중이면 거절한다.
- **Upgrade.** `devguard upgrade --release ID [--drain-timeout DURATION] [--stopped]`는 staged release 자신의 `devguard`로 실행해야 한다.
  1. 다른 wire version이나 protocol을 쓰거나, 다른 journal·설정 schema를 읽거나, 소비자가 요구하는 내구성 admission이나 fenced launch가 없는 release는 거절한다. 현재 release보다 적게 알리는 release는 downgrade로 보고하며 이 범위 안에서만 허용한다. 호환되지 않는 downgrade는 아무것도 바꾸기 전에 거절한다.
  2. 현재 release가 검증된 상태로 실행 중이어야 한다. Staged release의 복구 사본은 아무것도 바꾸기 전에 만든다.
  3. 실행 중인 서비스의 admission을 닫고 과금 중인 attempt나 lease가 없을 때까지 기다린다. Drain이 제한 시간(기본 60초) 안에 끝나지 않거나 SIGINT·SIGTERM으로 취소되면 현재 release에서 admission을 다시 열고, 그 release가 모든 과금을 유지하며, 아무것도 교체하지 않는다. 서비스를 멈추기 전에 받은 signal도 admission을 다시 연 채 upgrade를 끝낸다. 멈춘 뒤부터는 교체가 끝까지 진행되거나 되돌려지며, `launchctl`은 별도 process group에서 실행되므로 terminal의 interrupt가 닿지 않는다. C11 이전 release는 admission을 닫을 수 없다. `--stopped`는 그 release를 먼저 멈추고, journal에 과금된 것이 없을 때만 진행한다. 그렇지 않으면 그 release가 다시 제공한다.
  4. 서비스를 멈춘다. Authority lock을 보유한 채 `backups/<time>-<from>-to-<to>/` 아래에 quiescent 백업을 만든다. 완전한 SQLite 사본인 journal, 선택, 현재 manifest를 각각 hash와 함께 둔다.
  5. 닫기 marker를 기록하고 새 release를 시작하므로, 새 release는 admission이 닫힌 채 시작한다. launchd가 그 release 자신의 `devguardd`를 실행하는지, 소비자 handshake가 성공하는지, 서비스가 admission이 닫혀 있고 과금된 것이 없다고 보고하는지 검증한다.
  6. 새 release를 current로 기록하고 교체된 release를 last known good으로 남긴 뒤 admission을 다시 연다.

  Drain이 끝난 뒤 어떤 단계가 실패하면 이전 release가 같은 journal로 다시 제공하며 admission을 다시 연다. 멈추기가 실패해 아직 제공 중인 release는 admission만 다시 연다. 그렇지 않으면 새 release가 시작되었을 경우 아직 실행 중이든 검증 중에 죽었든 이를 bootout하고, endpoint와 lock이 비면 이전 release를 다시 시작한다. Release가 admission했을 수 있는 journal 위에 백업을 복원하지 않는다.

  새 release가 시작된 뒤 중단된 upgrade는 같은 upgrade를 다시 실행하면 완료된다. 선택이 그 release를 아직 가리키지 않으면, upgrade가 시작한 대로 닫힌 유휴 상태로 제공 중일 때에만 기록하고, admission이 아직 닫혀 있으면 다시 연다. 제공 중인 last known good release는 repair에 맡긴다. `devguard admission --open`은 제공 중인 선택된 release의 admission을 다시 연다. C11 이전 release에는 그 release가 읽지 않는 marker만 지운다.
- **Repair.** `devguard repair --use last-known-good`는 서비스를 last known good release로 되돌린다. 이는 마지막 upgrade가 교체한 release이며, upgrade가 없었다면 설치된 release이다. 어느 `devguard`로 repair를 실행하든 서비스는 그 release 자신의 바이너리만 실행한다.
  - 어떤 authority든 제공 중이면 거절한다. 두 번째 authority를 시작하지 않으며, 제공 중인 release는 upgrade로 교체한다. 유일한 예외는 중단된 repair의 완료다. Last known good release가 이미 검증된 상태로 제공 중이지만 아직 선택되지 않았으면, repair는 이를 기록하고 admission을 다시 연다.
  - Journal은 authority lock 아래에서 열려야 한다. 열 수 없으면 admission은 닫힌 채로 남고, repair는 journal을 만들거나 초기화하거나 복원하지 않는다.
  - 그 release는 journal의 schema를 읽고 같은 소비자를 제공해야 한다. Journal에 해제되지 않은 lease가 있으면 부모 lease가 없는 release는 거절한다.
  - 설치된 release가 손상되었으면 복구 사본을 대신 실행하며, status와 이후 upgrade도 이를 받아들인다.
  - 남아 있는 job이 endpoint와 lock을 놓으면 교체하고 실행 중인 바이너리를 검증한다. Load되지 않은 서비스의 bootout은 `launchctl`이 3으로 끝나더라도 성공으로 처리한다. Release가 제공하는 즉시 repair를 선택 기록에 남기고, 그다음 중단된 upgrade가 닫아 둔 admission을 다시 연다.
- **범위.** Upgrade에는 유휴 서비스가 필요하다. 실행 중인 작업은 끝날 때까지 기다리며 중단하지 않는다. C11 이전 release에는 `upgrade` 명령이 없다. 그 release로 돌아갈 때는 서비스를 멈춘 뒤 그 release 자신의 installer를 쓰며, 그 release는 닫기 marker를 무시한다.

## SLO 판정 (C12)

DG1-C12는 기능 시험의 성공으로 응답성을 추론하지 않고 release를 측정한다. Harness는 `devguard-qualify`(`crates/qualify`, release 패키지에 들어가지 않는다)와 `scripts/measure.py`이다. 설치된 서비스 자신의 release, 그 정책과 그 호스트를 측정하며, worktree 바이너리는 측정 대상 artifact가 되지 않는다.

- **제어 probe.** `devguard-qualify control`은 `devguard`처럼 경로나 authority를 바꾸는 옵션 없이 정규 authority의 평범한 `dev-cli` owner로 등록한다. 자신이 소유한 attempt만 측정하며, 대상은 작은 `/bin/sleep` target(50 mCPU, 16 MiB, task 2개)이다. 각 target은 admit·commit한 뒤 측정 대상 release의 `devguard-launch`로 시작하며, reap하기 전에 관측한다.
  - 상태 표본은 실행 중인 target을 새로 등록한 session(connect, hello, authenticate, register, lookup)으로 `Lookup`하는 것이다. 명령행 owner가 매 호출을 하는 방식대로 1초마다 한 번 잰다.
  - 종료 표본은 20초마다 새 target을 시작하고 `Terminate(SIGTERM)`부터 `Terminated`까지를 잰다. 신호 전달은 해제 증거가 아니므로 target의 종료와 attempt의 해제는 따로 보고한다.
  - 용량이나 압력 때문에 거절된 admission은 다시 시도하고 기록하지만 지연 표본은 아니다.
  - SIGINT나 SIGTERM은 표본 채취를 끝내지만, probe는 자신이 시작한 모든 target을 정리한다.
- **전경 fixture.** Fixture는 고정 로컬 페이지 `crates/qualify/fixture/foreground.html`이며, 모든 보고서에 hash를 기록한다. 새 profile의 Google Chrome에서 `--remote-debugging-pipe`로 구동하므로 수신 port가 없고, scheduling이나 throttling을 바꾸는 flag도 쓰지 않는다.
  - **입력.** 브라우저가 `Input.dispatch*`로 합성한다. 500 ms ± 100 ms마다 key, click, wheel 한 단계 중 하나를 보낸다.
  - **입력부터 다음 paint까지.** Event의 timestamp부터, 입력으로 생긴 화면 변화 뒤의 다음 frame까지를 잰다. 다음 frame은 다음 animation frame에서 보낸 message로 표시한다. 브라우저가 Event Timing duration을 보고하면 그 값까지 올려 잡는다. 브라우저 이전의 운영체제 입력 경로는 포함하지 않는다.
  - **Frame 정지.** 연속한 animation frame 사이가 500 ms를 넘는 경우다.
- **부하.** 소비자 6개가 측정 대상 release의 `devguard exec --wait`로 동시에 실행하므로 admission·압력·대기열은 서비스 자신의 것이다. 어떤 부하도 admission을 우회하지 않으며 모든 실행의 receipt를 보존한다.

  | 소비자 | 작업 | 예산 |
  | --- | --- | --- |
  | Cargo | release 자신의 source(`--adapter cargo-pipeline`): workspace build 뒤 core·contract 시험 | Cargo 기본값 |
  | CPU | thread 2개 | CPU 2개, 256 MiB, task 8개 |
  | 메모리 | 1.5 GiB를 쓰고 유지 | 250 mCPU, 2 GiB, task 4개 |
  | I/O | 512 MiB를 쓰고 sync한 뒤 다시 읽고 삭제 | 250 mCPU, 256 MiB, task 4개 |
  | 출력 압력 | 표준 출력으로 256 MiB | 250 mCPU, 128 MiB, task 4개 |
  | 느린 입력 | 100 ms마다 한 줄씩 읽기 | 100 mCPU, 64 MiB, task 4개 |

  Probe의 target까지 더한 동시 예산은 로컬 호스트의 작업 용량(5,500 mCPU, 11.75 GiB, task 144개) 안에 들어간다.
- **Protocol.**
  - 조합은 두 가지다. `cold`는 Cargo build마다 새 target directory를 쓰고, `warm`은 반복 전에 build한 target directory를 다시 쓴다.
  - 조합마다 반복 세 번을 실행한다. 각 반복은 idle 10분 뒤 최소 30분의 부하다.
  - 부하 기한 전에 시작한 작업은 끝날 때까지 관측하고, 기한 뒤에는 아무것도 시작하지 않는다.
- **백분위수와 누락 표본.** p99는 한 구간의 원시 값에 대한 nearest-rank 값이다.
  - 누락 표본은 모든 값보다 위에 둔다. 처리되지 않았거나 paint되지 않은 입력, 응답 없는 상태 호출이나 종료가 이에 해당한다.
  - 누락된 입력은 1초를 넘는 응답으로도 센다.
- **유효성.** 다음을 모두 만족해야 구간이 유효한 관측이다.
  - fixture가 headful로 실행되고 계속 전면 application이다.
  - 페이지가 계속 보이고 focus를 유지하며, visibility나 focus가 바뀌지 않는다.
  - 화면이 잠기지 않는다.
  - 서비스가 내내 측정 대상 release를 실행한다.
  - 모든 지표가 일정의 90% 이상(종료는 80%)을 가진다.
  - admit된 부하가 부하 구간의 절반 이상 동안 실행된다.

  `caffeinate`로 화면을 깨어 있게 한다. 유효하지 않은 구간은 `inconclusive`이며 결코 통과가 아니다.
- **판정.** 모든 목표를 지킨 구간만 통과한다.
  - 상태: p99 ≤ 500 ms, 연결 유실 없음.
  - 종료 확인: p99 ≤ 1초, 연결 유실 없음, 신호를 받은 모든 target 해제.
  - 입력부터 다음 paint까지: p99 ≤ 100 ms, 1초 초과 없음.
  - Frame: 정지 없음.
  - 부하: 불확실한 실행, 연결 유실, 중복 launch 없음.
  - 충전된 채 남은 것 없음.

  Idle 기준선과 부하가 모두 통과해야 반복이 통과하며, idle 기준선이 실패하면 그 반복은 inconclusive다. 세 반복이 모두 통과해야 조합이 qualified이고, 값을 반복 사이에서 합치지 않는다. Rehearsal(짧게 줄인 구간)이나 headless fixture는 언제나 inconclusive다.
- **승격.** `measure.py promote`는 전체 protocol의 qualified 요약만 받는다. Run header와 release manifest가 바뀌지 않았고, 서비스가 여전히 그 release를 실행해야 한다.
  - 비공개 상태 directory에 `qualifications/<release>.json`을 쓴다. 이는 읽기 전용 `devguard-release-qualification/v1` 기록이다.
  - 기록에는 release, 그 manifest와 artifact hash, 정책, 환경, harness와 계획, 모든 반복의 판정, 증거 hash가 들어간다.
  - 그 뒤 서비스가 그 release를 실행하는지 검증한다. Release manifest는 바꾸지 않는다. `slo_qualified: false`는 승격이 아니라 패키지를 기술한다.
- **한계.**
  - 이 판정은 측정한 호스트·artifact·정책에서의 독립 제어, 개발, 제한된 자기 적용만 보증한다. 다른 호스트나 release는 따로 측정해야 한다.
  - Fixture 통과가 모든 website를 보장하지는 않는다.
  - Linux 강제와 CodeSpace 결합은 판정하지 않는다.

## 내구성 admission과 launch

Journal schema는 1이다. 초기화는 명시적으로 새 파일만 만든다. 누락·손상·미지원 schema·불일치 journal은 fail-closed다. SQLite는 WAL, FULL synchronous와 immediate transaction을 사용한다. 시작 시 모든 저장 record와 회계 index를 대조한다.

Attempt key는 `(consumer_id, consumer_generation, attempt_id)`다. 요청 fingerprint는 버전 있는 실행 digest와 자원 intent를 포함하고 transport ID·현재 policy revision을 제외한다. Replay는 최초 내구 예약·종결 결과를 반환하고 의미나 owner 변경은 충돌한다. 거절도 종결 attempt이므로 나중에 별도로 요청하는 admission은 새 attempt ID를 사용한다.

begin_launch는 Prepared를 내구성 있게 소비하고 일회성 Secret permit을 반환한다. 반복 호출에는 새 spawn 권한이 없고 기존 attempt만 있다. Journal에는 permit digest만 저장한다. 첫 응답이 유실되면 대조해야 하며 transition replay로 helper를 다시 만들 수 없다. `verify_launch`는 grant를 바꾸지 않고 검사한다. `claim_launch`는 bind하기 전에 처음 도달한 helper의 수립된 scope를 내구성 있게 기록하며, 그 뒤에는 다른 scope가 claim·bind·authorize될 수 없고 `bind_scope`도 claim된 scope만 받는다.

bind_scope는 attempt·owner·정확한 프로세스 정체성·scope·전체 적용 plan을 연결한 신선한 Backend 증거를 요구한다. 여기와 대조에서 backend 증거의 신선도는 backend가 반환한 뒤 읽은 시계를 기준으로 판단한다. 따라서 transition 도중 관측한 증거를 미래로 취급하지 않으며, 2초보다 오래되었거나 다른 boot의 증거는 계속 거절한다. C04는 실제 시계 증거로 드러난 이 순서를 바로잡았으며, 고정 시계를 쓰는 가짜 backend로는 드러나지 않았다. authorize_run은 연결된 helper 정체성과 permit을 검증하며 최초 성공 응답만 may_exec=true다. 이 응답 유실은 불확실·차단된 실행이지 재생성 허용이 아니다. RunAuthorized는 사용자 executable 성공의 증거가 아니다.

Prepared 취소는 종결하고 예약을 반환한다. Commit 후 취소는 Draining으로 전환하고 늦은 bind·authorize를 막으며 예약을 유지한다. Draining·Suspect·종결 attempt를 취소해도 아무것도 바뀌지 않으며, 기한이 지난 Prepared attempt는 만료를 보고한다. Prepared만 5초 후 만료된다. 재시작은 Prepared의 원래 boot-relative 기한을 유지하고 commit 후 미종결 attempt와 등록 instance를 Suspect로 전환하여 대조한다.

## 회수 증거

무조건 release(lease_id) API는 없다. Bound 실행은 정확한 scope와 신선한 root 종료·reap, scope 비움, 알려진 구성원 생존 없음, 완전한 추적과 알려진 이탈 없음이 필요하다. 이전 추적 상실은 평범한 빈 group 관측만으로 사라지지 않는다. 명시 대조 증거나 검증된 호스트 reboot 종료가 필요하다.

Unbound committed launch는 PID 누락이나 owner 사망만으로 회수하지 않는다. Backend가 helper 미생성과 모든 pending spawn 부재를 적극적으로 입증해야 하며, macOS에서는 대조 절에서 설명한 owner 보고가 그 증거다. Reboot로 이전 실행 종료를 대조할 수도 있다. release_reason은 scope 종료·helper 미생성·이전 boot 종료를 구분한다. known_not_started는 prelaunch 종결 거절과 Released/NoHelperCreated에서만 참이다. 자원 반환 자체는 재시도 증거가 아니다.

일반 TTL sweep은 tombstone을 삭제하지 않는다. 운영자 generation 폐기에는 모든 instance retired와 charge 없음이 필요하다. 종결 attempt 압축 전에 영구 retired generation을 기록하여 과거 key를 계속 거절한다.

정적 제어 예약은 단절·offline을 포함한 모든 설정 슬롯에서 차감한다. Instance retire는 슬롯 재사용만 허용하고 정적 예약을 workload로 반환하지 않는다. 프로세스 정체성에는 boot ID·PID·start ticks가 있어 PID 재사용을 검출한다.

Journal은 instance의 등록 정책을 기록한다. Active/suspect instance가 남은 동안 소비자 삭제나 generation·role·UID·instance 상한·예약 변경은 시작 시 거절한다. 기존 설정에서 먼저 대조·retire한다. 슬롯 수는 generation을 가로질러 합산하므로 설정 재시작으로 같은 정적 예약에 두 번째 서비스를 배정하지 못한다.

## 압력과 capability

최초 유효 sample 전에는 closed다. 최초 정상 sample은 준비를 열지만 압력·관측 실패 후에는 설계의 30초 단계 복귀를 따른다. 같은 boot-relative monotonic clock을 사용하고 replay·미래 timestamp를 거절하며 6초가 지나면 stale이다. 목표 감소로 live lease 금액을 줄이지 않는다.

자원마다 level·method가 있다. Accounting은 OS 메모리 상한이 아니고 QoS는 메모리·task 제한이 아니며 kernel 제어에는 contained cgroup이 필요하다. Admission 전에 plan을 검증하고 AppliedResources와 구분한다. 호환성은 protocol과 모든 필수 capability를 함께 확인하며 fallback authority를 시작하지 않는다.

## 후속 마일스톤의 책임

- 남은 DG-1: 측정한 macOS SLO와 측정된 release의 승격(C12).
- CS-RG: Runner 슬롯·전송 lane, 승인 migration, client pin, 상태 결합과 회귀 qualification.
- DG-LINUX: 실제 cgroup 계층·controller·ancestor와 sandbox·proxy 포함.
- DG-CACHE·DG-ADAPTERS: 등록 cache 회수와 추가 도구 제어.

Journal·라이브러리는 사용자 프로세스 handle·출력·CodeSpace workspace lease·approval row를 소유하지 않는다. 자원 예약 수명으로 이들의 수명을 추정하지 않는다.
