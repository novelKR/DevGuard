# DevGuard 서비스 경계 운영

[English](../operations.md) | [한국어](operations.md)

C01은 명시 bootstrap·고정 저장소를, C02는 인증된 로컬 transport를, C03은 native macOS boot·프로세스·호스트 압력 증거를, C04는 협조적 정책 readback과 scope 증거를, C05는 fenced launch helper를, C06은 대조를, C07은 `devguard` 명령행 owner를, C08은 Cargo adapter를, C09는 패키지로 만든 release를 현재 사용자 LaunchAgent로 설치하는 기능을, C10은 부모 lease와 후보 authority를 제공한다. PR과 병합 후 main 전달 증거는 구현과 별도로 추적한다. devguardd serve는 이 증거로 journal을 활성화하는 foreground 서비스를 실행하고 wire 등록·fenced launch·대조를 연다. `devguard exec`는 이 서비스를 통해 명령을 실행하고, `devguard doctor`는 이를 진단한다. 정상 authority는 현재 macOS만 지원하며 Linux CI는 이식 가능한 계약과 제한된 fixture를 검사한다. DG-LINUX 제어 자격을 부여하지 않는다.

## 제공 명령

Bootstrap에서는 Rust 1.95.0과 단일 Cargo job을 사용한다.

```sh
CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-daemon -p devguard-launch -p devguard-cli
target/debug/devguardd paths
target/debug/devguardd init
target/debug/devguardd check
target/debug/devguardd serve
target/debug/devguard doctor --require admission,macos-cooperative
target/debug/devguard exec --wait 30s -- /usr/bin/true
```

`paths`는 운영 계정의 경로를 조회한다. `init`은 최초 bootstrap에만 쓰며 새 journal, 운영자 설정, 분리된 CLI·관리 자격을 생성한다. 기존 상태는 거절하고 덮어쓰지 않는다. `check`는 저장소 배타 소유권과 설정·journal을 검사하며 `runtime_ready: false`를 보고한다. Boot clock을 읽거나 복구·적용 완료를 주장하지 않는다. 다른 소유자가 잠금을 보유하면 두 번째 authority를 얻을 수 없다.

`serve`는 기존 journal을 검증하여 열고 배타 소유권을 얻은 뒤 정상 private UDS endpoint를 연다. Ctrl-C나 SIGTERM으로 foreground 프로세스를 중지한다. 종료는 세션과 자기 socket inode만 정리하며 journal·lock·자격은 보존한다. 남은 socket은 배타 authority lock을 얻고 연결 시도가 명시적인 connection refused를 반환하며 inode가 그대로일 때만 제거한다. 살아 있거나 busy이거나 관측할 수 없는 endpoint는 지우지 않는다.

Native 증거를 사용할 수 있으면 `serve`는 관측한 호스트로 정책을 산출하고, 실제 시계와 프로세스 정체성으로 journal의 boot 인지 복구를 수행한다. 이후 2초마다 호스트 압력을 sampling한다([계약](contracts.md#native-macos-호스트-증거) 참조).
- **Sampling 볼륨.** 상태 볼륨과 등록된 모든 project root다. 등록된 root가 없거나 읽을 수 없으면 모든 읽기가 실패하고 새 작업은 닫힌 상태로 남는다. 오래된 등록은 건너뛰지 않으므로 고치거나 제거한다.
- **시작 상태.** 압력은 Critical로 시작하며 첫 유효 sample에는 두 번의 읽기가 필요하다. 서비스 시작 시 호스트가 메모리 warning을 보고하면, 압력 규칙에 따라 메모리가 30초 동안 normal일 때까지 Critical을 유지한다. 그 뒤 Constrained가 되고, 다시 30초 뒤 Normal이 된다.
- **Receipt.** 서비스는 stderr에 JSON line을 남긴다.
  - 활성화 시 `native_host`, native 증거를 열 수 없으면 `native_host_unavailable`
  - `pressure_baseline`
  - 상태가 바뀔 때와 1분마다 `pressure`
  - controller가 sample을 거절할 때 `pressure_sample_rejected`
  - 상태 변화 없이 loop가 200 ms 이상 늦게 깨어날 때 `pressure_control_lag`
  - 첫 읽기 실패와 실패가 이어지는 동안 1분마다 `pressure_observation_failed`
  - sampler가 비정상적으로 끝나면 `pressure_stopped`
  - [계약 문서](contracts.md#대조)에 나열한 launch·대조 receipt

  Receipt에는 호스트 counter, 압력 상태, 상태·등록 project 경로와 그 mount point가 들어간다. 자격 정보는 포함하지 않는다.

Native 증거가 없는 플랫폼이거나 macOS 관측이 실패하면 `serve`는 저장소만 보유한 닫힌 동작을 유지하고 어느 경우인지 보고한다.

인증된 status는 storage_validated, registration_ready, execution_ready, 이유와 설정 fingerprint를 보고한다. Native 증거가 있으면 등록과 실행이 준비된 상태이며, admission은 여전히 호스트 압력과 용량을 따른다. 그렇지 않으면 둘 다 false이며 이유는 미지원 플랫폼과 native 관측 실패를 구분한다. `devguard`는 자기 옆의 `devguard-launch`를 찾으므로 둘을 함께 빌드한다. 설정이나 handshake 성공만으로 명령이 관리되지는 않으며 bootstrap 빌드는 자기 적용 증거가 아니다. 영속 서비스를 제거 가능한 target 경로에서 실행하지 않는다. 대신 패키지를 설치한다.

영속 서비스는 설치된 release를 실행한다.

```sh
python3 scripts/package.py --offline
target/package/<release-id>/bin/devguardd install --package target/package/<release-id>
devguardd status
```

`package.py`는 깨끗한 tree에서 release 패키지를 만든다. `install`은 그 패키지에서 실행해야 하며, authority가 제공 중이거나 agent가 있거나 기존 authority 상태가 없으면 거절한다. Release를 변경 불가능한 `releases/<release-id>`로 복사하고, 현재 사용자 LaunchAgent `io.github.novelkr.devguard`로 시작한다. 이 agent는 `releases/<release-id>/bin/devguardd serve`를 실행한다. launchd가 정확히 그 바이너리를 실행한다고 검증한 뒤에만 release를 선택한다. `devguardd status`는 설치된 서비스를 보고하며, 검증된 current release를 실행하지 않으면 1로 끝난다. Release 교체는 upgrade이며 repair와 함께 후속 작업(C11)이다. [계약 문서](contracts.md#설치와-현재-사용자-서비스)를 참조한다.

후보 build는 실행 중인 서비스의 부모 lease 안에서 검증한다.

```sh
devguard test-candidate --candidate /path/to/candidate/tree --report /path/to/new/report --wait 5m
```

`test-candidate`는 lease 하나(기본값 CPU 2개, 4 GiB, task 96개, 기한 2시간)를 admission하고, tree의 build, 해당하는 시험, tree 자신의 `devguardd candidate`를 `devguard exec --lease`로 lease 자식으로 실행한다. 후보의 admission을 바깥에서 점검하고 lease를 끝낸 뒤 새 보고 디렉터리에 `report.json`을 쓰며, 모든 단계가 통과했을 때만 0으로 끝난다. 서비스는 부모 lease를 광고해야 하므로 C10 이후 release가 필요하며, 압력이 Critical인 동안에는 lease를 admission하지 않는다. [계약 문서](contracts.md#부모-lease와-후보-authority)를 참조한다.

`devguard exec [--project ID] [--adapter auto|generic|cargo|cargo-pipeline] [--wait DURATION] [--cpu MILLICPU] [--memory SIZE] [--tasks N] [--receipt PATH] [--lease CONSUMER/GENERATION/ATTEMPT --lease-token-fd N] -- PROGRAM [ARGS...]`는 명령을 admission하고 `devguard-launch`로 시작한 뒤 끝날 때까지 기다린다. 프로그램과 인자는 `--` 뒤에 두며 shell은 사용하지 않는다. 명령의 종료값으로 끝나거나 그 signal로 끝나며, 아무것도 시작하지 않았으면 125로 끝난다. `--wait`가 없으면 거절 즉시 끝나고, `--wait`가 있으면 용량·압력 거절을 기한까지 다시 시도하며, 호스트의 작업 용량보다 큰 요청은 즉시 거절한다. `--receipt`는 private JSON receipt를 기록한다. `--lease`는 명령을 그 부모 lease의 자식으로 만들며, descriptor N에서 읽은 token으로 lease의 남은 예산에 대해 admission한다. `devguard doctor`는 JSON 진단을 출력하며, `--require admission,registration,macos-cooperative`를 주면 요구 사항이 하나라도 충족되지 않을 때 실패한다. [계약 문서](contracts.md#명령행-owner)를 참조한다.

`--adapter cargo`는 `cargo`의 compiler job을 예약에 맞추고, `--adapter cargo-pipeline`은 검증 script 같은 프로그램이 실행하는 Cargo들이 jobserver 하나를 공유하게 한다. 기본값인 `auto`는 compile하는 `cargo` 명령에 Cargo adapter를 고른다. Cargo job 하나(CPU 1개와 2 GiB)도 수용할 수 없는 예약은 거절한다. [계약 문서](contracts.md#cargo-adapter)를 참조한다.

## 정상 소유권과 경로

정상 경로는 OS 계정 데이터베이스에서 결정한다. caller의 HOME·XDG·socket·state override와 무관하다. root·setuid 실행은 거절한다. 운영용 대체 경로나 부모 없는 시험 예산 인자는 없다. 후보 경로에는 후속 C10 부모 예산 계약이 필요하며, 격리 fixture 경로는 시험 빌드 안에서만 생성한다.

| 용도 | macOS 경로 |
| --- | --- |
| 운영자 설정 | `~/.config/devguard/host.toml` |
| 영속 원장 | `~/Library/Application Support/DevGuard/state/authority.sqlite` |
| 영속 배타 잠금 | `~/Library/Application Support/DevGuard/state/authority.lock` |
| 등록·관리 자격 | `~/Library/Application Support/DevGuard/credentials/` 아래 별도 파일 |
| runtime endpoint | `/private/tmp/devguard-<uid>/authority.sock` |
| 후속 cache | `~/Library/Caches/DevGuard/` |

DevGuard 디렉터리는 0700, 파일은 0600이며 소유자·종류·symlink를 검사한다. 부모 경로 이동, 안전하지 않은 상위 디렉터리, 링크된 private 파일과 공유 권한은 거절하고 사용자 경로를 자동 chmod하지 않는다. 공유 sticky tmp는 private runtime 디렉터리의 상위 경로로만 허용한다. journal·lock은 tmp 밖에 둔다.

잠금은 O_NONBLOCK으로 열고 열린 descriptor의 metadata로 현재 UID 소유·private 일반 파일·링크 하나를 명시 초기화 때도 확인한다. FIFO·hard-link fixture는 blocking이나 authority 소유권 획득 없이 실패해야 한다.

일반 시작에는 기존 영속 디렉터리·journal·lock이 필요하다. 누락·손상·미지원 상태를 자동 초기화하지 않는다. 명시 bootstrap의 부분 실패는 진단과 명시 repair를 위해 보존한다. init 재실행은 repair가 아니다. 시작을 통과시키려고 활성 lock·journal·자격·복구 artifact를 제거하지 않는다.

## 설정 권한

운영자 파일은 크기가 제한된 UTF-8 TOML schema 1이며 알 수 없는 필드를 거절한다. 정책 revision, interactive profile, 소비자의 generation·자격 digest·역할·최대 인스턴스·정적 제어 예약과 프로젝트 root를 정의한다. 평문 자격은 별도 private 파일에만 둔다. 모든 consumer·관리 자격 digest는 서로 달라야 하며 workload 역할은 제어 예약을 부여할 수 없다. 파서 오류에 설정 원문이나 자격을 출력하지 않는다.

최초 `dev-cli`는 인스턴스 8개를 허용하고 인스턴스마다 제어 예약을 추가하지 않는다. 승인된 CLI 관제 풀은 실제 회계가 제공될 때 중앙에서 한 번 계산한다. task 설정의 초기값은 용량 256, 호스트 여유 64, 시스템 예약 48의 **회계 추정치**다. system_tasks는 제한된 session worker 32개와 서비스·관제 여유 task 16개를 포함하여 최소 48이어야 한다. 운영자가 설정하는 상한이며 macOS kernel 제한이나 측정된 충분성이 아니다. C12에서 선택한 값을 기록·검증해야 한다. CPU·메모리 용량은 관측한 호스트에서 얻는다. 여유분과 제어 예약은 [계약](contracts.md#native-macos-호스트-증거)의 승인 기본값을 따른다. 추가 여유분은 용량을 줄일 수만 있다.

이전 system_tasks = 16 기본값을 사용하는 C01 설정은 C02에서 거절한다. 설정 schema는 1을 유지하며 이는 더 엄격한 의미 검증이지 자동 호환성이나 migration이 아니다. C02 시작 전에 운영자는 전체 task 용량·여유분·모든 예약을 검토하고 해당 용량 안에서 system_tasks >= 48을 명시 선택해야 한다. 기존 journal·자격을 보존한다. 검증을 통과시키려고 init을 다시 실행하거나 호스트 용량을 자동 확대하거나 live 예약을 줄이면 안 된다.

프로젝트 `.devguard.toml`은 schema, project ID, profile, adapter와 더 낮은 Budget 상한(cpu_milli, memory_bytes, tasks)만 가진다. 자격·역할·별도 authority·소비자 사칭·호스트 용량은 지정할 수 없다. 프로젝트 상한은 운영자 상한에 들어가야 하며 미지원 필드·adapter·버전은 거절한다. 프로젝트별 설정은 authority core에 들어가지 않는다.

```toml
schema = 1
project_id = "devguard-dev"
profile = "interactive"
adapter = "cargo"

[limits]
cpu_milli = 1000
memory_bytes = 2147483648
tasks = 32
```

이 예시는 실행이 이미 lease를 소비한다는 증거가 아니다. `devguard exec --project ID`를 쓰려면 프로젝트의 절대 root를 운영자 설정에 등록하고 그 안에서 실행해야 한다. 프로젝트나 자격은 추가 호스트 예산을 만들지 않는다.

## Client 인증과 transport 제한

Client library는 정상 endpoint에 연결하고 OS가 관측한 authority UID/PID를 확인하며 wire version 1과 handshake의 자기 UID/PID를 대조한다. Consumer ID·generation·secret으로 설정된 workload/control-service 역할을 인증하고 별도 secret으로 관리 역할을 인증한다. UID 일치만으로 역할을 얻지 않는다. Helper permit은 caller 자격으로 사용할 수 없고 동일 UID의 악의적인 프로세스를 격리하지 않는다.

Wire는 4-byte 길이, JSON payload 최대 64 KiB, 활성 세션 최대 32개와 frame read/write마다 절대 250 ms 기한을 사용한다. 다음 frame을 기다리는 idle 시간도 포함하므로 idle 연결은 만료된다. 나중에 별도 작업을 명시적으로 시작할 때 새 인증 세션을 사용한다. Client가 자동으로 재접속하거나 재시도하지는 않는다. Poll·nonblocking descriptor I/O는 부분 frame·느린 reader·마지막 응답 버퍼를 처리하고 peer 종료 뒤 Darwin timeout 옵션을 바꾸지 않는다. 응답 유실로 실행·미실행·자원 회수를 입증했다고 판단하지 않는다.

Private-FD API는 시작 metadata에 descriptor 식별자만 전달한다. Receiver는 이후 exec 전에 자격 FD를 소비하고 닫으며 실제 subprocess로 이 경계를 시험한다. Secret을 argv·환경·debug·payload 상속 descriptor에 두면 안 된다. 이 시험은 C05 helper 권한이나 사용자 프로그램 시작의 증거가 아니다. OS UID/PID 관측을 제공하며 C03은 authority 안에서 이를 native boot/start 정체성과 결합한다. Native 증거가 있으면 인증된 소비자는 관측된 자기 프로세스를 instance로 등록하고, 그다음 admission과 일회성 launch grant를 받을 수 있다. Grant는 호출자가 아니라 그 helper가 제시한다.

SDK 조회 예제는 CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-client --example inspect로 빌드한다. 인터페이스는 inspect SOCKET UID CONSUMER GENERATION CREDENTIAL_FD다. 부모가 전용 상속 FD로 secret byte를 제공해야 하며 인자는 FD 번호와 비밀이 아닌 연결 metadata만 전달한다. 관측 peer와 인증된 status를 출력하는 예제이며 `devguard` 실행 CLI가 아니다.

## 호환성·검사·복귀

Core의 AuthorityStorage는 기존 배타 잠금을 보유하고 schema 1 journal을 검사하되 attempt 상태를 바꾸지 않는다. 실제 Backend·Clock으로 활성화할 때 boot 기반 복구와 같은 transaction 안에서 회계를 다시 검증한다. 기존 Authority::open의 복구 동작과 DG-0 시험을 유지하며 C01–C06은 journal schema와 contract 직렬화를 바꾸지 않는다. C05와 C06은 서비스가 fenced launch를 광고한 뒤에만 client가 사용하는 wire 요청과 응답을 추가한다. C10은 서비스가 부모 lease를 알린 뒤에만 client가 사용하는 lease 요청과 응답, 그리고 없을 때 만드는 `leases`·`lease_children` table을 추가하며 schema는 바꾸지 않는다. 새 로컬 protocol은 미지 필드·버전·필수 capability 미지원을 엄격히 거절하며 필드 추가도 명시적 호환성 시험이 필요하다.

```sh
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
python3 scripts/qualify.py dg1-probes --offline
python3 scripts/qualify.py dg1-scopes --offline
python3 scripts/qualify.py dg1-launch --offline
python3 scripts/qualify.py dg1-reconcile --offline
python3 scripts/qualify.py dg1-cli --offline
python3 scripts/qualify.py dg1-cargo --offline
python3 scripts/qualify.py dg1-bootstrap --offline
python3 scripts/qualify.py dg1-self-use --offline
python3 scripts/validate.py --offline
```

기능 suite는 0개 실행을 실패로 처리하고 source fingerprint·toolchain·bootstrap 모드·로그를 남긴다. dg1-authority는 배타 시작, 경로 별칭·권한, 누락·손상·미래 journal, 활성화 사이의 회계 손상, 엄격한 설정 버전, 프로젝트 권한 상승, 동시 bootstrap, 관측한 호스트 용량에서 산출한 정책을 검사한다. dg1-auth는 실제 peer 관측·인증 역할·엄격한 frame·제한된 통신·private FD 위생과 등록이 닫힌 상태의 동시 요청을 검사한다.

`dg1-probes`는 macOS에서만 실행하며 다른 플랫폼은 `not_run`으로 기록한다. 검사 항목은 다음과 같다.
- `kern.bootsessionuuid`와 대조한 boot 시계
- 종료·zombie·부재·거부 경우를 포함한 반복 가능한 프로세스 정체성
- 호스트 용량과 구조적으로 유효한 native 압력 읽기
- 첫 sample 전과 sample 없이 6초가 지난 뒤의 닫힌 admission
- probe 실패 시 즉시 닫힘
- stale·미래·replay·다른 boot sample 거절
- 관측한 socket peer의 native 등록
- 주입한 실패와, authority 잠금을 붙잡으면 안 되는 kernel 안에서 멈춘 probe를 포함한 서비스 sampling loop

각 native 단계는 선언한 raw receipt를 보고서의 `raw/` 디렉터리에 남겨야 하며 단계 로그는 hash로 기록한다. 환경이 만들 수 없는 경우는 `not_run`으로 기록하며, 그러면 suite는 `passed`가 아니라 `incomplete`가 된다. `--allow-incomplete`는 보고서를 바꾸지 않고 incomplete suite에 성공을 반환한다. 환경이 해당 경우를 만들 수 없다고 알려진 곳에서만 사용한다.

`dg1-scopes`도 macOS에서만 실행한다. 자기 process group을 이끌고 utility QoS clamp로 다시 실행한 실제 scope root를 사용하며 검사 항목은 다음과 같다.
- nice·QoS readback과 clamp 없는 root의 적용 실패
- binding, 실행 권한, 모든 구성원이 끝난 뒤에만 이루어지는 회수
- 자손이 남은 상태에서 root가 종료·reap되어도 회수하지 않음
- 자손이 group을 떠난 뒤 고정되는 이탈
- 정체성을 확인한 종료
- 공유되거나 비어 있지 않은 group, 다른 사용자의 프로세스, kernel 요구, 알 수 없는 scope의 거절

Scripted-table 단위 시험은 생성 경쟁, PID 재사용, 추적 상실을 다룬다. Scope 증거는 라이브러리로만 검증하며 서비스는 P3 전까지 scope를 수립하지 않는다.

`dg1-launch`도 macOS에서만 실행한다. Launch를 연 격리 시험 authority와 합성 정상 호스트 probe로 실제 `devguard-launch` helper를 실행하며 검사 항목은 다음과 같다.
- READY 보고 뒤 helper가 argv·작업 디렉터리·환경·종료 상태를 유지한 executable로 바뀌고, scope가 끝난 뒤에만 회수되는지
- Executable의 descriptor는 표준 세 개뿐이며 permit·transcript·authority session이 없고, 인자와 환경에도 permit이 없는지
- Grant마다 claim된 helper가 하나뿐인지: replay한 commit에는 permit이 없고, 경쟁하는 두 helper 중 executable은 하나만 시작하며, 늦은 helper는 거절된다
- Owner가 만들지 않은 helper, 등록되지 않은 instance, 잘못된 permit이 claim 전에 거절되고 grant는 계속 쓸 수 있는지
- 취소 뒤에 도착한 helper가 차단되는지
- READY 뒤의 exec 실패가 거절과 구분되어 보고되는지
- 응답 유실 뒤처럼 같은 grant를 두 번 제시한 bare helper가 두 번째에는 실행하지 말라는 답을 받는지
- Utility clamp 없이 claim 뒤에 거절된 bare helper가 종료되고 scope로만 회수되는지. 환경이 모든 자식에 clamp를 걸면 이 경우는 `not_run`으로 기록한다.
- Authority가 바빠 helper의 250 ms 기한을 넘긴 응답이 실행으로 이어지지 않는지
- 여러 thread에서 동시에 launch해도 각 executable이 owner가 상속 가능하게 남긴 것만 상속하는지
- 이미 자기 session과 group을 이끄는 pseudo-terminal 위의 helper

Core·scripted-table·transcript·wire·session 시험은 claim transition, Suspect·Draining·종결 attempt를 바꾸지 않는 취소, helper 정체성 검사, 엄격한 transcript와 메시지, 다른 요청을 할 수 없는 helper session을 다룬다.

`dg1-reconcile`도 macOS에서만 실행한다. 격리 authority로 실제 helper를 실행하며, 그중 하나는 시험이 종료하고 다시 시작하는 자식 프로세스에서 동작한다. 검사 항목은 다음과 같다.
- Prepared 취소, 그리고 원래 기한까지 Prepared로 과금된 뒤 그 기한에 이루어지는 만료. 둘 다 시작하지 않았음이 알려진 상태다.
- Helper가 없다는 owner 보고(spawn 실패, READY 전에 종료한 helper, 유실된 grant 응답)가 `NoHelperCreated`로 회수되고 늦은 helper는 거절되는지
- 보고로 claim된 grant를 회수할 수 없는지
- 배경 reconciler를 멈춘 채 owner가 root를 reap하기 전에 관측할 때, 그 관측만이 살아남은 구성원을 편입하며 scope의 모든 구성원이 끝난 뒤에만 회수하는지
- 관측 전에 reap된 root의 살아남은 구성원이 추적 상실이 되어 attempt가 Suspect로 남고 회수되지 않는지
- 알려진 이탈이 모든 프로세스가 끝난 뒤에도 attempt를 Suspect로 유지하는지
- Scope의 모든 구성원을 종료하고 그 종료 뒤에 회수하는지
- Authorization 뒤의 취소가 scope가 끝날 때까지 예약을 유지하는지
- 죽은 owner의 claim되지 않은 grant가 Suspect가 되고 그 instance가 유지되는지, 과금 작업 없이 종료한 owner가 폐기되는지
- Prepared 기한이 지나도 시간만으로는 응답 없는 helper를 정리하지 않고 owner 보고로 정리하는지
- Daemon crash 전과 재시작 뒤의 과금 합계가 같고, commit된 모든 attempt가 Suspect이며, 늦은 helper는 거절되고, 추적을 잃은 실행 scope는 끝난 뒤에도 회수되지 않으며, owner 보고가 claim되지 않은 grant를 회수하는지
- Journal 쓰기 실패: bind 쓰기가 실패하면 claim된 helper를 exec 전에 멈추고 scope로 정리하며, 회수 쓰기가 실패하면 보고한 뒤 쓸 수 있을 때까지 다시 시도하는지

자식 프로세스 authority의 receipt는 보관하며, permit이나 호출자 자격이 들어 있지 않은지 확인한다.

Core·launcher 증거·서비스·wire 시험은 이전 boot 회수, 옛 PID를 누가 쓰든 이루어지는 이전 boot instance 폐기, release reason record 불변 조건, 쓰기 없는 대조, attempt·instance 목록, owner에 묶인 보고, owner 생존 규칙, 오염된 authority에서 서비스를 멈추는 reconciler, 엄격한 요청 decoding을 다룬다.

`dg1-cli`도 macOS에서만 실행한다. 먼저 `devguard-launch`를 빌드한 뒤, CLI를 별도의 owner 프로세스로 격리 authority에 연결해 실행한다. 검사 항목은 다음과 같다.
- 명령의 인자·작업 디렉터리·환경·종료값이 보존되고, scope가 끝난 뒤 회수되는지
- Signal로 끝난 workload가 CLI를 같은 signal로 끝내는지
- CLI에 보낸 SIGTERM·SIGHUP이 workload group의 모든 구성원에 도달하는지, 호출자가 무시한 signal은 계속 무시되고 전달되지 않는지
- CLI가 workload의 SIGSTOP을 반영하지 않는지
- Reap 전 관측으로 살아남은 구성원이 끝날 때까지 추적되고 과금되는지
- 호스트가 수용할 수 없는 예산이 아무것도 시작하지 않는지, 그런 예산의 대기가 attempt 없이 즉시 거절되는지
- 명시적 대기가 용량이 회수된 뒤 admission되는지, 기한에 끝나는지, signal로 취소되는지
- 사용할 수 없는 authority와 fenced launch capability가 없는 authority가 아무것도 시작하지 않는지, 요구 사항 유무에 따른 doctor 진단
- 등록된 프로젝트의 상한과 작업 디렉터리 포함 조건
- 동시 owner가 최대 8개인지, 끝난 owner가 pool을 소진하지 않는지
- 없거나 실행할 수 없는 프로그램, 이미 있는 receipt 경로, 없는 helper가 아무것도 admission하지 않는지
- Pseudo-terminal에서 출력만 terminal에 있는 경우를 포함해 interrupt 키가 workload에 바로 도달하는지, 정지가 반영되어 job control shell이 terminal을 되찾는지

인자·준비·진입점·scripted authority 시험은 엄격한 parsing, 프로그램 탐색, 예산 우선순위, 의미 digest, 유실된 admission·launch commit 응답, 대기의 재시도 간격·기한·취소, READY 전에 끝난 helper 뒤의 재시도 규칙, 끝까지 읽은 transcript, 실제 바이너리의 잘못된 호출을 다룬다.

`dg1-cargo`도 macOS에서만 실행하며 먼저 `devguard-launch`를 빌드한다. 작은 offline workspace를 CLI를 통해 실제 Cargo로 build한다. Compiler wrapper가 각 compile의 시작과 끝을 기록하고, `cargo` shim이 각 실행이 상속한 descriptor를 기록한다. 검사 항목은 다음과 같다.
- 예약된 job 수 안에서 이루어지고 target 디렉터리를 유지하는 direct build
- 예약에 맞춰 조정되거나 예약 안이라 유지되는 명시적 job 수
- 아무것도 시작하지 않는 경우: 충돌하는 job 수, 지원하지 않는 subcommand, job 하나에 못 미치는 예약, Cargo adapter에 지정한 Cargo가 아닌 프로그램. 그리고 아무것도 compile하지 않는 Cargo 명령에 `auto`가 generic adapter를 고르는지
- 시험 thread는 그대로 둔 채 예약 안에서 compile하는 `cargo test`
- 동시에 실행한 Cargo들이 FIFO jobserver 하나를 공유하고, 보고서 경로를 유지하며, 두 target을 모두 build하고, token을 돌려주는 Python pipeline
- 바깥 jobserver를 공유하는 build script 안의 중첩 Cargo
- 보존되어 build를 제한하는 상속 FIFO·descriptor 쌍 jobserver, Cargo 실행 전에 제거되는 오래된 상속 descriptor
- 각자의 예약 안에서 실행되는 동시 소비자
- Build가 멈추고 jobserver가 제거되는 취소된 pipeline

각 Cargo 실행과 pipeline script는 표준 descriptor만 상속한다. 호출자 자신의 descriptor 쌍 jobserver를 상속한 경우만 예외다. Shim은 각 Cargo가 받은 인자와 jobserver·fallback 변수도 기록한다. Hosted macOS 14 runner처럼 호스트의 작업 용량이 해당 경우에 필요한 Cargo job을 수용할 수 없으면 그 경우를 `not_run`으로 기록하고 suite는 `incomplete`를 보고한다. CI는 `--allow-incomplete`로 실행한다.

`dg1-bootstrap`도 macOS에서만 실행한다. 시험은 각 패키지의 `devguardd`로 시험 바이너리를 복사하므로, installer는 실제로 자기 패키지에서 실행된다. 설치된 서비스는 그 사본이며 격리된 fixture authority를 제공한다. 검사 항목은 다음과 같다.
- 선택 전에 실행이 검증된 설치 release, 변경 불가능한 release·복구 사본과 정상 status
- 같은 release의 재사용, 그리고 같은 id의 다른 release는 설치된 release를 건드리지 않고 거절하는지
- 아무것도 쓰기 전에 거절되는 패키지: 바뀌었거나 남거나 빠졌거나 symlink인 바이너리, 다른 capability, 맞지 않는 scope, 패키지 자신의 것이 아닌 installer
- authority가 제공 중이거나 authority 상태가 없을 때의 거절
- 끝내 자신을 입증하지 못해 아무것도 선택하지 않고 unload되는 서비스
- 서비스 하나만 남기는 동시 installer
- launchd에서 고유 label의 임시 job으로: 같은 journal로 다시 시작하는 crash, SIGTERM 정지, journal을 읽을 수 없어 닫힌 채 끝나고 다시 시작하지 않는 시작

launchd gui domain이 없는 session에서는 launchd 경우를 `not_run`으로 기록하고 suite는 `incomplete`를 보고한다.

`dg1-self-use`도 macOS에서만 실행하며 먼저 `devguard-launch`를 빌드한다. 격리 authority에 대해 실제 lease 자식과 실제 후보 authority 프로세스를 실행한다. 검사 항목은 다음과 같다.
- 호스트에 한 번 과금되는 lease, 남은 예산에 대해서만 admission되는 자식, lease를 넘지 않는 자식 합계
- 요구한 client에게만 알리는 부모 lease capability, token만 쓰는 보유자 session, 틀린 token이나 모르는 lease의 거절
- Lease가 끝났을 때, owner가 사라졌거나 이전 boot의 것일 때, 기한이 지났을 때의 fence, lease를 과금 상태로 유지하는 Suspect 자식, 모든 자식이 정리된 뒤의 해제와 재시작 뒤의 같은 동작
- workload를 lease 자식으로 실행하며 표준 descriptor만 상속시키는 `devguard exec --lease`, lease보다 큰 자식(대기 포함), 위조 token, lease가 끝난 뒤 시작하는 자식
- 용량 안에서 admission하고, 아무것도 launch하지 않고, lease를 갖지 않고, lease와 함께 닫히며, 부모 상태를 열지 않는 후보 authority, 그리고 위조 token, 모르거나 끝난 lease, lease보다 크거나 자기 예약 이하인 용량, 이미 있는 영역 때문에 거절되는 후보
- workload와 후보를 lease 하나의 자식으로 실행한 뒤 그 lease를 해제하고 후보 영역을 지우는 `test-candidate`, 그리고 제공 전에 죽어 실행은 실패하지만 scope와 lease는 해제되는 후보

이 suite들은 Linux 강제, 자기 적용, foreground SLO를 not_run으로 남긴다. 전체 검증은 기존 44개 시험과 workspace crate 8개의 명시적 전체 의존 그래프를 검사한다. Daemon은 자기 시험에서 fixture를 켜기 위해서만 자신에게 의존한다. Launch crate는 시험의 격리 authority를 위해서만 daemon에 의존한다. CLI는 daemon의 경로·설정과 Cargo adapter에 의존하며, daemon fixture에는 시험에서만 의존한다. Cargo adapter는 contract에만 의존한다. Core·contract는 daemon 설정과 native adapter에 독립적이며 client는 core에 의존하지 않는다.

복귀할 때 작업 소유 foreground 프로세스를 중지하고 보존한 설정과 schema 1 journal에 호환되는 source/artifact를 선택하며 영속 상태·자격을 유지한다. C03 활성화는 새 record 종류를 추가하지 않고 기존 복구만 수행하므로 C02 artifact로 같은 상태를 다시 열 수 있다. C06부터는 정상 서비스를 통해 workload가 시작될 수 있다. 대조가 없는 artifact로 복귀하기 전에 새 작업 시작을 멈추고 과금 중인 attempt가 종결 phase에 이를 때까지 기다린다. C05 artifact는 같은 journal을 읽지만 launch를 닫아 두고 대조하지 않으므로 남은 attempt는 과금된 Suspect로 남는다. Scope가 실행 중일 때 서비스가 멈추면 다시 시작한다. 그 attempt들은 owner가 claim되지 않은 grant를 보고하거나 reboot가 종료를 입증할 때까지 과금된 Suspect로 남는다. C07은 서비스 상태를 바꾸지 않는다. CLI를 되돌리려면 CLI로 명령을 시작하는 것을 멈춘다. 이미 시작한 명령은 scope가 끝날 때까지 과금되며, 관리되지 않는 실행으로 대체하지 않는다. C08도 서비스 상태를 바꾸지 않는다. Cargo job 조정을 멈추려면 `--adapter generic`을 명시하며, 기존 target과 cache는 유지된다. C09 installer는 release가 검증되기 전에는 job을 bootout하고 plist를 제거하며 아무것도 선택하지 않는다. 설치된 서비스를 멈추려면 `launchctl bootout gui/<uid>/io.github.novelkr.devguard`를 실행하고 plist를 지운다. Release·복구 사본·선택·journal은 보존되며, 보존한 artifact의 foreground `devguardd serve`가 같은 상태를 읽는다. C10은 lease table 말고는 서비스 상태를 추가하지 않는다. C09 artifact로 복귀하기 전에 모든 lease를 끝내고 해제되기를 기다린다. C09 서비스는 lease table을 무시하므로 lease의 쓰지 않은 남은 예산을 보유하지 않는다. `candidates/` 아래의 후보 영역은 버려도 되는 상태이며 정상 서비스는 읽지 않는다. 복귀나 재시작을 성공시키려고 journal이나 tombstone을 삭제하지 않는다.
