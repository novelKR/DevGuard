# DevGuard 서비스 경계 운영

[English](../operations.md) | [한국어](operations.md)

DG1-C01은 명시적 bootstrap, 고정 운영 경로, 설정·저장소 검사를 제공한다. 실제 실행은 아직 닫혀 있다. native 호스트 관측, 인증 transport, launch·대조는 별도 구현 단위다. 정상 authority는 현재 macOS만 지원하며 Linux CI는 이식 가능한 계약과 제한된 fixture를 검사한다. DG-LINUX 자격을 부여하지 않는다.

## 제공 명령

Bootstrap에서는 Rust 1.95.0과 단일 Cargo job을 사용한다.

```sh
CARGO_BUILD_JOBS=1 cargo build --locked -p devguard-daemon
target/debug/devguardd paths
target/debug/devguardd init
target/debug/devguardd check
```

`paths`는 운영 계정의 경로를 조회한다. `init`은 최초 bootstrap에만 쓰며 새 journal, 운영자 설정, 분리된 CLI·관리 자격을 생성한다. 기존 상태는 거절하고 덮어쓰지 않는다. `check`는 저장소 배타 소유권과 설정·journal을 검사하며 `runtime_ready: false`를 보고한다. 가짜 boot clock을 만들거나 복구·적용 완료를 주장하지 않는다. 다른 소유자가 잠금을 보유하면 두 번째 authority를 얻을 수 없다.

`serve`, 일반 명령 실행, 설치, LaunchAgent와 repair는 후속 작업에서 제공한다. 설정만으로 명령이 관리되지는 않으며 필수 bootstrap 빌드는 자기 적용 증거가 아니다.

## 정상 소유권과 경로

정상 경로는 OS 계정 데이터베이스에서 결정한다. caller의 HOME·XDG·socket·state override와 무관하다. root·setuid 실행은 거절한다. 운영용 대체 경로나 부모 없는 시험 예산 인자는 없다. 후보 경로에는 후속 C10 부모 예산 계약이 필요하며, 격리 fixture 경로는 시험 빌드 안에서만 생성한다.

| 용도 | macOS 경로 |
| --- | --- |
| 운영자 설정 | `~/.config/devguard/host.toml` |
| 영속 원장 | `~/Library/Application Support/DevGuard/state/authority.sqlite` |
| 영속 배타 잠금 | `~/Library/Application Support/DevGuard/state/authority.lock` |
| 등록·관리 자격 | `~/Library/Application Support/DevGuard/credentials/` 아래 별도 파일 |
| runtime endpoint | `/private/tmp/devguard-<uid>/authority.sock` (C02에서 transport 제공) |
| 후속 cache | `~/Library/Caches/DevGuard/` |

DevGuard 디렉터리는 0700, 파일은 0600이며 소유자·종류·symlink를 검사한다. 부모 경로 이동, 안전하지 않은 상위 디렉터리, 링크된 private 파일과 공유 권한은 거절하고 사용자 경로를 자동 chmod하지 않는다. 공유 sticky tmp는 private runtime 디렉터리의 상위 경로로만 허용한다. journal·lock은 tmp 밖에 둔다.

일반 시작에는 기존 영속 디렉터리·journal·lock이 필요하다. 누락·손상·미지원 상태를 자동 초기화하지 않는다. 명시 bootstrap의 부분 실패는 진단과 명시 repair를 위해 보존한다. init 재실행은 repair가 아니다. 시작을 통과시키려고 활성 lock·journal·자격·복구 artifact를 제거하지 않는다.

## 설정 권한

운영자 파일은 크기가 제한된 UTF-8 TOML schema 1이며 알 수 없는 필드를 거절한다. 정책 revision, interactive profile, 소비자의 generation·자격 digest·역할·최대 인스턴스·정적 제어 예약과 프로젝트 root를 정의한다. 평문 자격은 별도 private 파일에만 둔다. 관리와 등록 자격은 달라야 하며 workload 역할은 제어 예약을 부여할 수 없다. 파서 오류에 설정 원문이나 자격을 출력하지 않는다.

최초 `dev-cli`는 인스턴스 8개를 허용하고 인스턴스마다 제어 예약을 추가하지 않는다. 승인된 CLI 관제 풀은 실제 회계가 제공될 때 중앙에서 한 번 계산한다. task 설정의 초기값은 용량 256, 호스트 여유 64, 시스템 예약 16의 **회계 추정치**다. 운영자가 설정하는 상한이며 macOS kernel 제한이나 측정된 충분성이 아니다. C12에서 선택한 값을 기록·검증해야 한다. CPU·메모리의 여유분과 제어 기본값은 승인 설계를 유지하고 C03에서 실제 용량을 사용한다. 추가 여유분은 용량을 줄일 수만 있다.

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

이 예시는 실행이 이미 lease를 소비한다는 증거가 아니다. 후속 CLI 도입 전에 실제 절대 프로젝트 root를 운영자 설정에 등록한다. 프로젝트나 자격은 추가 호스트 예산을 만들지 않는다.

## 호환성·검사·복귀

Core의 AuthorityStorage는 기존 배타 잠금을 보유하고 schema 1 journal을 검사하되 attempt 상태를 바꾸지 않는다. 실제 Backend·Clock으로 활성화할 때 boot 기반 복구와 같은 transaction 안에서 회계를 다시 검증한다. 기존 Authority::open의 복구 동작과 DG-0 시험을 유지하며 C01은 journal·contract wire 형식을 바꾸지 않는다.

```sh
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/validate.py --offline
```

기능 suite는 0개 실행을 실패로 처리하고 source fingerprint·toolchain·bootstrap 모드·로그를 남긴다. 배타 시작, 경로 별칭·권한, 누락·손상·미래 journal, 활성화 사이의 회계 손상, 엄격한 설정 버전, 프로젝트 권한 상승과 동시 bootstrap을 검사한다. Native launch·Linux 강제·자기 적용·foreground SLO는 not_run이다. 전체 검증은 기존 44개 시험을 유지하고 daemon/TOML 계층의 명시적 의존 그래프를 추가한다. Core·contract는 서비스·설정 의존성과 독립적이다.

복귀할 때 작업 소유 foreground 프로세스를 중지하고 호환되는 이전 source/artifact를 선택하며 영속 상태·자격을 보존한다. C01을 통해 시작한 workload는 없다. 후속 live lease의 복귀에는 실제 대조가 필요하므로 이 초기 빈 상태 가정을 재사용할 수 없다.
