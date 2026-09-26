# 후속 설계 결정과 기준 상태

문서 기준일: 2026-09-22, ADR-006 추가 2026-09-27. 이 문서는 승인된 독립 설계의 후속 선택을 기록한다. 선택의 승인과 기능의 구현·qualification은 별개다. 아래 선택은 확정되었지만 DG-1 이후 구현은 모두 미착수다.

## 기준과 문서 권한

| 대상 | 고정 기준 | 해석 |
| --- | --- | --- |
| DevGuard 구현 | [`d59cbd43d206a9a9281328a946eddf1dc199f710`](https://github.com/novelKR/DevGuard/tree/d59cbd43d206a9a9281328a946eddf1dc199f710) | DG-0 계약·core·가짜 backend 시험 |
| CodeSpace 런타임 | [`e94d21475643608ad2a466256fb57266b86faa47`](https://github.com/novelKR/CodeSpace/tree/e94d21475643608ad2a466256fb57266b86faa47) | 초기 결합 설계의 이력 분석 기준; DevGuard 미결합 |
| CodeSpace 확인 기준 | [`b6e7ed22e2c730ac987297455e250cbd6e8e8b0c`](https://github.com/novelKR/CodeSpace/tree/b6e7ed22e2c730ac987297455e250cbd6e8e8b0c) | 설계 개정 1의 기준; patch helper의 시험 재사용 외에는 runtime 경로가 `e94d214`와 같음; DevGuard 미결합 |
| CodeSpace 기존 로드맵 | `fb822fc24c98f6628dce62d33a5cc67275f8ca34` | 이번 연결 문서 PR에 포함할 기존 로컬 문서 commit |
| DG-1 완료 후 DevGuard | [`395315d34b5d458ea1774446727f0cb14bd8a120`](https://github.com/novelKR/DevGuard/tree/395315d34b5d458ea1774446727f0cb14bd8a120) | DG-1 완료 뒤 결합 계획의 기준 |
| 개정 대상 DevGuard PR #8 head | `92a34721d88f39a22cdde4603958d6c447c90e76` | 설계 개정 1이 수정한 문서 head |
| Codex pin | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` | 설계 개정 1이 유지; 이후 변경은 검증에 근거한 별도 결정 |
| Codex 비교 snapshot | `b334d5b3f2d9441b95286a8c2af8c2152737d977` | 2026-09-27 검토에서 본 upstream `main`; 배포 pin이나 자동 채택 대상이 아님 |
| 설계 개정 1 | [design-revision-1.md](../design-revision-1.md) | 2026-09-27에 채택한 CS-RG 실행 소유권·재사용의 현재 기준 |
| 승인 설계 | [design.ko.md](../../design.ko.md), [출처](../../design-source.json) | 원문과 checksum 보존 |
| 설계 SHA-256 | `97b67a1f9518c1781156a4b3b26829b285f84f5c9a44da60f3c5dcf1bc768df8` | 문구 정정도 원문 수정 대신 이 문서에 기록 |
| 라이선스 | [Apache-2.0](../../../LICENSE) | CodeSpace와 동일한 라이선스; 기존 LICENSE·NOTICE 보존 |

로컬 개발 위치는 `/Volumes/DevData/Projects/IdeaProjects/DevGuard`다. 문서 층위를 구분한다: 역사적 승인본은 당시 결정을 보존하고, 설계 참조는 [설계 개정 1](../design-revision-1.md)처럼 사용자가 지시한 개정을 적용한 현재 편집 기준이며, `docs/contracts.md`는 실제 구현된 동작만 기술하고, `milestones.json`은 마일스톤 ID·의존·상태의 원본으로서 실제 구현·검증 진행에 따라서만 바뀐다. 상세 작업·시험·PR 경계는 각 마일스톤 문서가 소유한다. 문서 revision은 계획을 인용하는 값이며, 미래 소비 제품의 client dependency pin이나 설치 artifact 승격을 의미하지 않는다. 설계를 개정해도 CS-RG가 구현이나 qualification 상태가 되지는 않는다.

## ADR-001 — 실행 소유자인 Runner의 단일 등록

**결정: 채택. 분류: Required. 근거 신뢰도: 높음.** 승인 설계 §2.2, §4.3의 서비스 등록 모델을 CodeSpace의 현재 소유 구조에 맞게 구체화한다.

관찰 근거는 DevGuard `crates/core/src/authority.rs`의 `TrustedPeer`와 등록 PID 검증, CodeSpace `crates/runner/src/process.rs`의 프로세스 handle·PTY 소유권이다. DG-0 등록은 소비자 자격뿐 아니라 OS가 관측한 peer PID와 등록 identity의 PID가 일치해야 한다. Gateway가 worker의 PID를 대신 선언하는 방식은 이 계약에 맞지 않는다.

| 대안 | 장점 | 비용·한계 | 결정 |
| --- | --- | --- | --- |
| Runner 단일 등록 | 기존 PID 검증과 실제 실행 소유자가 일치; 등록·lease 주체 하나 | Gateway 비용까지 정적 예약에 산정하고 worker에 자격을 안전하게 전달해야 함 | 최초 채택 |
| Gateway 등록 후 worker가 같은 Principal 사용 | 시작 코드는 단순해 보임 | peer PID와 실행 소유자가 달라짐; 자격·종료 책임 불명확 | 채택하지 않음 |
| 서비스 등록 + 하위 worker 등록 | 다중 Runner·공유 서비스 예약을 표현하기 쉬움 | 부모/자식 등록·예산 이전·부분 장애·generation 계약 추가 | 실제 수요 발생 시 재검토 |

InProcess는 Gateway 내부 Runner가 같은 PID로 등록한다. UDS는 worker가 등록한다. Gateway와 Runner의 제어 비용은 하나의 정적 예약에 함께 포함한다. worker별 전체 호스트 예산이나 Gateway용 추가 전체 예산을 만들지 않는다. 명시한 최대 인스턴스 슬롯을 넘으면 거절한다.

**현행 규칙.** 실행 소유자당 instance 하나를 등록하며 한정된 세션마다 같은 instance를 다시 등록한다. 따라서 “단일 등록”은 실행 소유자당 instance 하나를 뜻한다. 별도의 `service-exec` 경로는 없다. Gateway는 소비자 자격을 `CredentialHandoff`로 UDS worker에 넘기고 InProcess는 직접 읽는다. launcher는 두 번째 instance를 등록하지 않으며, SDK 없는 일반 서비스의 등록 방식은 별도 지원 계약이 마련될 때 정의하고 PID 검증을 완화하지 않는다. 최소 변경은 Runner의 작은 client adapter와 시작 경로, private 자격 FD 전달이다. DevGuard core에 CodeSpace process_id·PTY·workspace 정책을 넣지 않는다. payload 이전에 자격 FD를 닫고 pipe·PTY를 모두 시험하며, 공개 MCP 인자에 FD 번호나 등록 자격을 추가하지 않는다. Runner가 관리 실행을 생성·관찰·회수하는 방식은 [ADR-006](#adr-006--codespace-실행-소유권과-재사용-정책)이 정한다.

호환성 영향은 새 operator 설정과 capability, private client/Runner 시작 계약이다. 구현 비용은 중간이며 인증·FD 누출·정적 예약 과소 산정이 주요 위험이다. 정상 등록, 잘못된 UID/PID, 재사용 PID, 두 모드, FD 누출, 동일 슬롯 중복 시작을 검증한다. rollback은 신규 admission을 닫고 실행·lease를 대조한 뒤 소비 설정을 이전 검증 조합으로 돌린다. 실행 중 자격 또는 journal을 지워 회계를 초기화하지 않는다.

재검토 조건은 한 서비스가 동시에 여러 독립 Runner를 사용하거나 여러 서비스가 제어 예약을 공유해야 하는 실제 배치다. 그때 하위 등록의 생애·generation·예산 귀속을 별도 결정으로 설계한다.

**대체된 문구(이력).** 결정은 유지한다. 2026-09-26의 DG-1 구현 보충과 ADR-006이 원래 문구의 두 부분을 대체했으며, 아래에는 이력으로만 남긴다.
- “SDK를 사용하는 CodeSpace에서 `service-exec`는 자격 FD 전달과 서비스 시작을 준비하며, 선택된 Runner가 실제 등록을 완료한다.” DG-1에는 `service-exec` 경로가 없다. 현행 규칙을 따른다.
- “`crates/pty/src/lib.rs`는 현재 상속 FD 목록에 빈 배열을 전달하지만 고정된 Codex PTY 구현은 선택 FD 상속을 지원한다. CodeSpace adapter를 확장하고 [...] 이 목적만으로 Codex pin을 변경하지 않는다.” 고정된 spawn 함수는 child를 내부에서 회수하고 이미 상속 가능한 descriptor만 유지하므로 adapter 확장으로는 관리 실행을 담을 수 없다. 실행 경로와 pin 결정의 범위는 ADR-006이 정한다.

[CodeSpace 결합 명세](codespace-integration.md#등록과-시작의-단일-소유자)와 [CS-RG](milestones/CS-RG.md#dg-1-소비-인터페이스)를 참조한다.

## ADR-002 — 살아 있는 독립 Runner에 대한 Gateway 복구

**결정: 채택. 분류: Required. 근거 신뢰도: 높음.** 승인 설계 §2.1의 책임 분리, §3.4의 연결 소실 정책, §5.2의 P1-RECOVERY를 구체화한다.

현재 CodeSpace의 `crates/server/src/runtime.rs`는 worker child를 소유하고 기존 연결 종료 정책에 따라 정리한다. `crates/runner/src/process.rs`의 handle·PTY·입출력은 메모리에 있다. DB에 PID를 적는 것만으로 이 소유권을 재생성할 수 없다.

| 대안 | 복구 범위 | 비용·위험 | 결정 |
| --- | --- | --- | --- |
| 독립 Runner 생존 + Gateway 재연결 | 같은 프로세스·PTY·입출력 소유자에 재연결 | 운영자 모드, 인증, epoch와 제어권 fence, 상태 대조 필요 | 최초 채택 |
| Runner도 재시작하고 입출력 복원 | Runner 손실까지 확대 | 별도 장수 I/O 소유자·spool·handle 이전·보존 정책 필요 | 별도 후속 설계 |
| 저장 argv 자동 재실행 | 새 프로세스를 만들 수 있음 | 부수 효과 중복; 원래 실행과 동일하지 않음 | 복구 수단으로 금지 |

새 복구 모드는 운영자 선택과 명시적인 capability로 도입한다. 현재 InProcess·managed UDS·기존 연결 모드의 종료 계약을 바꾸지 않는다. InProcess는 Gateway와 같은 PID이므로 live 복구 대상에서 제외한다. 기본 설정을 바꾸는 rollout은 별도 검토 대상이다.

새 독립 Runner 모드에서 정상 Gateway 종료는 명시된 종료 의도를 전달한다. 명시적 서비스 중지는 Runner가 실행을 drain/종료하고 실제 증거로 lease를 대조하는 흐름이다. 재시작용 detach와 예기치 않은 연결 소실에서는 Runner가 기존 timeout·출력 상한·프로세스를 계속 관리한다. 정상 종료와 detach를 신호 하나로 추정하지 않으며 운영 설정과 메시지 계약으로 구분한다.

재연결은 인증된 새 Gateway에 Runner epoch와 제어권을 발급하고 이전 Gateway의 mutation을 fence한다. 같은 실행에 재연결하는 데 새 작업 예산을 요구하지 않는다. workspace 점유·approval·attempt·lease는 동일 identity로 대조하며 원래 deadline을 연장하지 않는다. Runner/호스트 손실은 불확실 상태로 보존하고 실제 종료 여부를 확인한다. PID/DB만으로 PTY·pipe 복원 성공을 보고하지 않는다.

최소 변경은 독립 Runner lifecycle, process 전용 영속 기록, 인증 재연결, 관측·승인·workspace 대조다. patch operations 원장과 분리한다. 비용은 높으며 split-brain, 오래된 승인, 출력 유실이 주요 위험이다. Gateway 반복 재시작·동시 재연결·stale epoch·Runner 손실을 검증한다. rollback은 새 모드의 신규 시작을 중지하고 기존 Runner를 drain한 후 이전 모드로 복귀한다. 기존 실행을 버리고 argv를 재생하는 방식은 허용하지 않는다.

## ADR-003 — 정상 authority 하나와 상위 예산 안의 시험

**결정: 채택. 분류: Required. 근거 신뢰도: 높음.** 승인 설계 §2.3, §4.4, §4.5에 대응한다. DG-0 잠금은 journal 부모 디렉터리 단위다. 이는 라이브러리 경계의 보호이며 임의 디렉터리마다 전체 호스트 용량을 할당해도 된다는 의미가 아니다.

DG1-C01은 정상 서비스의 canonical 경로·권한·소유자·잠금을 고정한다. 시험용 다른 경로는 DG1-C10에서 상위 lease의 예산과 자격에 종속된 후보 authority에만 허용한다. socket/state override를 다른 전체 호스트 authority의 우회로로 제공하지 않는다. 원격 실행은 실제 executor 호스트의 authority를 소비한다.

대안인 각 checkout별 독립 governor는 중앙 보장을 충족하지 못한다. 모든 시험을 정상 journal에 섞는 방식도 실패 주입과 복구 경계를 훼손한다. 선택은 정상 단일 authority + 격리된 bounded 후보다. 비용은 중간이며 상위 lease 만료·부모 손실의 fence가 핵심 위험이다. 경로 별칭·동시 시작·부모 손실 시험으로 검증한다. 복귀는 후보를 정리하고 부모 회계를 대조하는 절차이며 정상 journal 재초기화가 아니다.

최초 bootstrap에서 기능 시험을 통과해 동결한 기준 artifact와 전체 개발·foreground SLO까지 통과한 안정 artifact를 구분한다. 처음부터 존재하지 않는 안정 버전을 요구하는 순환을 만들지 않는다. 기준 artifact에는 기능 시험 범위와 제한된 후보 시험 권한을 표시하고 일상 사용 자격을 부여하지 않는다. upgrade/repair는 고장 난 후보의 admission 없이 실행 가능한 보존된 복구 경로와 제어 예약을 사용한다.

## ADR-004 — 호환성과 관제 보호의 실제 경계

**Required / 신뢰도 높음:** journal 및 공개 계약의 엄격한 역직렬화(`deny_unknown_fields`)를 고려해 구·신 reader/writer를 실제 fixture로 검증한다. 필드 추가를 자동 후방 호환으로 가정하지 않는다. source/client pin, daemon/helper artifact, CodeSpace wire의 세 축을 별도로 확인한다. schema 변경이 필요하면 읽기/쓰기 호환·migration·downgrade 정책을 해당 구현 PR에 포함한다. 새 admission 이후 오래된 DB snapshot으로 rollback하지 않는다.

**Required / 신뢰도 높음:** CodeSpace의 현재 spawn 후 슬롯 검사, 직렬 wire dispatch, 공유 writer, 완료 응답 중심 replay는 실제 결합 전 개선한다. 슬롯을 spawn 전에 확보하고 control/data의 대기 수·처리 수·누적 byte·보관 시간까지 제한한다. 소켓만 둘로 나누어 공유 mutex/callback이 관제를 계속 막는 구현은 완료가 아니다. 정상과 포화·느린 stdin·대량 출력·응답 유실을 시험한다. 위험은 실행/승인 중복과 수명 누수이며 CSRG-C03~C09의 PR 경계로 준비·정리·검증과 backend 결정을 묶는다.

**Strongly Recommended / 신뢰도 높음:** CodeSpace `crates/runner/src/files.rs`의 전체 파일 read/hash 뒤 window 적용 경로는 별도 메모리 상한 개선 대상이다. 근본 원인은 응답 크기와 내부 할당량이 다른 것이다. 파일 크기 제한의 명시적 거절은 작은 해결책이고 streaming hash/read는 더 넓은 기능을 유지하는 대안이다. 소비자 API의 오류·hash 일관성을 검토해야 하며 비용은 중간이다. 큰 파일·동시 읽기·파일 변경 시험과 peak memory로 검증한다. 이 작업은 이번 runtime 범위에 추가하지 않고 후속 승인 대상으로 남긴다. 해결 전 qualification workload에서 파일 크기·동시성을 제한하고 임의 크기 파일까지 관제가 보호된다고 광고하지 않는다.

## 구현 순서와 재검토 규칙

우선 경로는 DG-0 → DG-1 → CS-RG → P1-RECOVERY다. DG-1은 독립 CLI/daemon·개발 workload·자기 적용을 검증하고, CS-RG는 결합된 Runner·승인·replay·관제 포화를 검증한다. DG-1 완료에 아직 없는 CS-RG를 요구하지 않는다. DG-LINUX는 제품 전체 완료에 필수다. DG-CACHE와 추가 adapter는 P1-RECOVERY의 선행 조건이 아니다.

위 결정이 새 증거로 변경될 때에는 영향받는 작업 ID·계약·시험·migration을 이 문서의 새 결정으로 기록한다. 기존 승인 설계, 과거 검증 증거, 완료 범위를 소급 수정하지 않는다. 이번 문서 PR은 선택을 기록하며 새 runtime API·상태 schema·dependency를 적용하지 않는다.

## ADR-005 — 언어·순차 전달·운영 소유권

**채택 / Required / 높은 신뢰도.** 영문 정본과 PR 본문, 검토한 한국어 번역을 유지하고 source/번역 hash를 검사한다. 승인 원문과 기존 commit/link는 보존한다. DevGuard #1에 추가 commit으로 영문화를 반영하고 CodeSpace #65를 실제 immutable 문서 revision으로 갱신한다. #1을 먼저 검증·병합·main 확인하고 #65의 main CI와 문서 배포까지 확인한 뒤 P1을 시작한다.

각 codex/dg1-p1~p6은 검증된 main에서 별도 worktree로 진행하며 현재 head 검토/CI, 정상 exact-head 병합, main CI, 증거 복사·hash 검증과 task 소유 output/worktree/local branch 정리까지 끝낸 뒤 다음을 시작한다. 원격 branch, 원래 CodeSpace checkout/index, toolchain/journal/reference/recovery artifact는 보존한다. P1~P4는 foreground, P5는 현재 사용자 LaunchAgent이며 특권 daemon이 아니다. 재시작은 기존 journal을 열어 대조하며 누락·손상 시 fail-closed다.

C08 기능 bundle을 target 밖에 보존하고 C09 설치/복구 경계를 만든다. C10에서 새 부모 예산 기능을 먼저 시험한 뒤 이를 포함한 부모를 동결한다. 이전 artifact의 지원을 가정하지 않는다. 그 직후 별도 후보를 부모로 실행하고 이후 해당 build/test를 실제 자기 적용하며 receipt를 보존한다. C12의 SLO 승격과 기능 부모를 구분한다. 새 범위·권한·소유권·호환성·workload 가정·합격 기준이 달라질 때만 추가 결정을 요청한다.

## ADR-006 — CodeSpace 실행 소유권과 재사용 정책

**결정: 2026-09-27 사용자의 명시적 지시로 채택. 분류: Required.** 전체 명세는 [설계 개정 1](../design-revision-1.md)에 있고 [CodeSpace 결합 명세](codespace-integration.md#실행-소유권)가 이를 적용한다. 이 결정은 설계 준수 여부가 아니라 설계가 지금도 적절한지를 재평가한 결과다. 기존 결정은 무엇을 바꾸고 무엇을 다시 검증해야 하는지 알려 주는 정보이며 대안을 기각하는 근거가 아니다.

고정된 Codex 고수준 spawn은 회수 소유권과 FD 전달 계약이 달라 변경 없이 DG-1 관리 실행을 담을 수 없다(F1a, F1b). 같은 발견에 동시 spawn(F1c), 출력 bridge의 손실과 Drop 동작(F1d), 무기한 남을 수 있는 중복 backend(F1e)가 포함된다. 증거 수준은 고정 revision의 코드 검토이며 개정 문서의 부록에 정리했다.

| 결정 | 대안 | 근거와 비용 | 결정 |
| --- | --- | --- | --- |
| D1: `required` 실행 | 현재 pin의 CodeSpace 소유 Unix transport(A1); 변경 없는 고정 고수준 spawn; 새 Codex pin; 제한적 adaptation(A4); `ProcessDriver` | 고정 spawn은 내부에서 회수하고 상속 가능한 descriptor만 유지; 최신 upstream API는 snapshot 비교만 수행; 고정 `ProcessDriver`는 lag 출력을 건너뛰고 Drop 시 종료; A1도 master/slave 수명·session 설정·resize·실패 정리·출력·shutdown을 유지해야 함 | 기본은 A1. A4는 아래 정책을 따름. `ProcessDriver`는 출력·backpressure·Drop 기준을 만족할 때만. pin 변경은 검증에 근거한 별도 결정 |
| D2: legacy `off` backend | 통합 후 제거; 제한적 compatibility backend 유지; 무기한 보류 | 보류하면 수명주기와 spawn 보호를 두 벌 유지; 어느 결과든 유지보수 비용을 입증 | CSRG-C08 전에 CSRG-C09에서 결정 |
| D3: DevGuard의 Codex 의존 | C0: 의존 없음; adapter에서 저수준 유틸리티의 조건부 재사용 | `HelperCommand`·launcher·native 관측 중 대체되는 부분이 입증되지 않음 | 지금은 C0; 아래 트리거에서 재검토 |

**현행 규칙.** child당 회수 책임자는 하나다. `required` 경로에서는 소유 객체 밖의 어떤 코드도 `wait`, `try_wait`, `waitpid`를 호출하지 않으며 종료·timeout·shutdown은 supervisor에 의도를 보낸다. `helper_command`와 `HelperCommand::spawn`은 `spawn_guard`를 직접 잡으므로 호출자는 그 주위에서 guard를 잡지 않는다. 같은 프로세스의 다른 모든 child 생성 경로는 descriptor 생성·상속 설정·spawn 구간만 공통 guard나 검증된 동등 보호로 감싼다. 회수 전 `Observe`에는 전체 예산(1초 제안, CSRG-C00에서 검증)을 두며 실패해도 lease를 점유한 채 둔다. 준비 결과는 한 번만 소비한다. 출력은 CodeSpace 수집기 하나로 모으고 알 수 없는 손실을 `output_lost=false`로 보고하지 않는다.

**adaptation 정책(A4).** PTY 할당, terminal 설정, resize, 제한된 I/O 보조 코드처럼 명확히 분리된 실행 메커니즘에만 허용한다. `codex-core` 제품 의미, 세션 권한, Agent Loop, 광범위한 crate 복사, 의존성 검사를 회피하기 위한 복제는 허용하지 않는다. adaptation마다 원본 저장소·전체 SHA·파일 경로, 가져온 범위, 변경 이유, 의도적으로 달라진 동작, 대응 시험, upstream 갱신 시 재검토 조건, 제거·upstream 재수렴 조건을 기록한다. 호환되지 않는 의존성을 숨기기 위해 upstream crate를 복사하지 않는다는 CodeSpace의 기존 금지는 유지한다.

**의존 경계(D3).** `devguard-contract`, `devguard-core`, 범용 client 계약에는 Codex 제품 타입, 모델 세션, CodeSpace workspace 권한, PTY 소유권을 넣지 않는다. DevGuard의 기본 배포와 공용 client는 현재 Codex에 의존하지 않는다. 실행·플랫폼 adapter의 저수준 유틸리티 재사용은 실제로 대체하는 코드, 계약 적합성, 의존성 전파, 복구 경로, 재검증 비용을 평가해 결정한다. DG-LINUX 착수, 공개된 범용 FD attachment·외부 회수 소유 API의 등장, DevGuard의 child 감독 범위 확대, 같은 OS 결함의 반복 수정 때 재검토한다. 트리거는 비교를 다시 여는 조건이며 기능 이름만으로 의존성을 채택하지 않는다. 근거로 쓰는 의존성 수에는 SHA, target, feature, runtime/build/dev 구분, 실행 명령이 함께 있어야 한다. 검증기의 dependency-boundary 단계(`scripts/validate.py`)는 DevGuard graph의 모든 `codex-`·`codespace-` package를 거절하여 C0을 강제한다. adapter에서 그런 crate를 재사용하기로 결정하면 같은 PR에서 이 검사를 바꾼다.

적용 결과 CSRG-P1 앞에 CSRG-C00이, C07과 C08 사이에 CSRG-C09가 추가되어 CS-RG는 10개 작업·6개 묶음이 된다. 이 결정은 후속 구현을 구속하는 문서를 바꾸며 구현·qualification 상태는 바꾸지 않는다. rollback은 새 문서 revision이며 아직 이 결정에 의존하는 runtime 상태는 없다.
