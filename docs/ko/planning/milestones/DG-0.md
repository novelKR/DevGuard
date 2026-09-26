# DG-0 — 완료된 기반의 이행 기록

소유 저장소: DevGuard. 구현 상태: `implemented`. Qualification: 정확한 source에 대한 보고서 필요. 이 문서는 실제 초기 commit 하나를 분류해 설명하며 과거의 가상 작업 commit이나 PR을 만들지 않는다.

## 실제 이력과 범위

실제 commit은 [`d59cbd43d206a9a9281328a946eddf1dc199f710`](https://github.com/novelKR/DevGuard/commit/d59cbd43d206a9a9281328a946eddf1dc199f710), 제목은 `feat: establish DG-0 resource contracts and durable authority`다. 아래 DG0-R01~R06은 이행 기록 ID이며 후속 48개 예정 commit 수에 포함하지 않는다. 모두 이 하나의 commit에서 제공되었다.

| 이행 ID | 실제 산출물 | 해결한 문제와 보존한 계약 | 정상·실패·경쟁 시험 |
| --- | --- | --- | --- |
| DG0-R01 | `crates/contract/src/lib.rs`, `tests/public_contract.rs` | intent/reservation/plan/applied, 실행 digest, identity, compatibility 분리 | deterministic digest, overflow·unknown field 거절, policy 수준 오표시 거절 |
| DG0-R02 | `crates/core/src/policy.rs`, `authority.rs` | 정적 제어 예약 제외 후 admission; 최소 작업이 안 들어가면 거절 | 동시 소비자 합계, 중복 attempt, 0예산, 정책 축소 시 live lease 보존 |
| DG0-R03 | `authority.rs` 등록·generation | trusted peer·consumer 자격·정확한 PID 확인, 인스턴스 슬롯 공유 | UID만으로 등록 거절, 재접속, PID 재사용, active generation 변경 거절 |
| DG0-R04 | `journal.rs`, `authority.rs` | durable attempt·launch fence·tombstone, 재시작 Suspect | 응답 유실, commit/cancel 경쟁, journal 오류, 잠금, 회계 index 손상 |
| DG0-R05 | `pressure.rs`, `tests/pressure_contract.rs` | 첫 유효 probe 전 closed; 압력 상승·단계 회복 | stale/future/reboot sample, 관측 실패, 30초 회복, disk watermark |
| DG0-R06 | `scripts/validate.py`, CI, 설계·계약·ledger | 정확한 source/toolchain/dependency 증거를 남김 | checksum·의존 경계·fmt·clippy·44개 계약 시험, runtime `not_run` |

진입 조건은 승인 설계와 Apache-2.0 저장소 설립, Rust 1.95.0/Python 3.11 이상이었다. 초기 기준 workspace는 contract/core 두 crate다. `Backend`는 OS 증거를 받아들이는 추상 경계이며 시험 구현은 가짜다. transport 인증, daemon, helper, CLI, macOS 정책 적용, 실제 cgroup, 자기 적용, CodeSpace 런타임은 이행 범위 밖이다.

## 확인된 검증과 재현

[초기 macOS·Ubuntu CI](https://github.com/novelKR/DevGuard/actions/runs/35671367559)는 위 source에서 두 platform 모두 44개 시험을 통과했다. 구성은 public contract 7개, authority 30개, pressure 7개다. Ubuntu에서 통과한 것은 가짜 backend 계약이며 kernel controller qualification이 아니다.

기존 보존 증거는 로컬 ignored `evidence/dg0-independent-d59cbd4/report.json`, `evidence/ci-35671367559/dg0-macos-14-1/report.json`, `evidence/ci-35671367559/dg0-ubuntu-24.04-1/report.json`이다. 이 경로의 파일은 공개 배포물로 가정하지 않는다. CI run 및 artifact, 보고서의 source/head·tree digest·tests_passed·toolchain을 함께 확인한다. artifact 보존 기간이 끝나면 같은 source를 재검증해 새 run을 연결한다.

현재 제공되는 명령:

```sh
python3 scripts/validate.py --offline
```

Rust 1.95.0·rustfmt·Clippy와 Python 3.11 이상이 필요하다. locked dependency가 아직 없으면 `--offline`을 제거한다. validator가 한 Cargo job·한 test thread를 사용하며 새 ignored output에 source fingerprint와 로그를 기록한다. `--allow-toolchain-mismatch` 결과는 지정 toolchain qualification을 대체하지 않는다.

## 완료 증거와 다음 단계

완료 조건은 source/설계 checksum·license·dependency 경계 보존, 44개 계약 시험과 fmt/clippy 통과, report의 runtime 항목이 사실대로 `not_run`인 것이다. 문서 commit 위에서 재실행하면 새 source의 계약 회귀 증거이며 초기 CI나 OS qualification을 소급 변경하지 않는다.

rollback은 구현 소비자가 아직 없는 이 기준에서는 commit 비교와 새 checkout 재현으로 수행한다. 시험 journal은 시험별 격리한다. 미래 운영 journal이 생긴 뒤 이를 baseline 파일로 덮어쓰는 복구는 허용하지 않는다. 후속 DG1-C01에 계약/fixture/검증기와 남은 실제 OS 경계를 인계한다. 새 crate 추가 시 현재 두 crate를 열거하는 dependency 검증기를 같은 구현 PR에서 명시적으로 확장해야 한다.
