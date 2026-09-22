# 후속 설계 결정과 기준 상태

문서 기준일: 2026-09-22. 이 문서는 승인된 독립 설계의 후속 선택을 기록한다. 선택의 승인과 기능의 구현·qualification은 별개다. 아래 선택은 확정되었지만 DG-1 이후 구현은 모두 미착수다.

## 기준과 문서 권한

| 대상 | 고정 기준 | 해석 |
| --- | --- | --- |
| DevGuard 구현 | [`d59cbd43d206a9a9281328a946eddf1dc199f710`](https://github.com/novelKR/DevGuard/tree/d59cbd43d206a9a9281328a946eddf1dc199f710) | DG-0 계약·core·가짜 backend 시험 |
| CodeSpace 런타임 | [`e94d21475643608ad2a466256fb57266b86faa47`](https://github.com/novelKR/CodeSpace/tree/e94d21475643608ad2a466256fb57266b86faa47) | 결합 지점 분석의 기준; DevGuard 미결합 |
| CodeSpace 기존 로드맵 | `fb822fc24c98f6628dce62d33a5cc67275f8ca34` | 이번 연결 문서 PR에 포함할 기존 로컬 문서 commit |
| Codex pin | `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` | 유지; DevGuard 문서 revision과 별개 |
| 승인 설계 | [design.ko.md](../design.ko.md), [출처](../design-source.json) | 원문과 checksum 보존 |
| 설계 SHA-256 | `97b67a1f9518c1781156a4b3b26829b285f84f5c9a44da60f3c5dcf1bc768df8` | 문구 정정도 원문 수정 대신 이 문서에 기록 |
| 라이선스 | [Apache-2.0](../../LICENSE) | CodeSpace와 동일한 라이선스; 기존 LICENSE·NOTICE 보존 |

로컬 개발 위치는 `/Volumes/DevData/Projects/IdeaProjects/DevGuard`다. `docs/contracts.md`는 현재 구현 계약, `milestones.json`은 마일스톤 ID·의존·상태의 원본이다. 상세 작업·시험·PR 경계는 각 마일스톤 문서가 소유한다. 문서 revision은 계획을 인용하는 값이며, 미래 소비 제품의 client dependency pin이나 설치 artifact 승격을 의미하지 않는다.

## ADR-001 — 실행 소유자인 Runner의 단일 등록

**결정: 채택. 분류: Required. 근거 신뢰도: 높음.** 승인 설계 §2.2, §4.3의 서비스 등록 모델을 CodeSpace의 현재 소유 구조에 맞게 구체화한다.

관찰 근거는 DevGuard `crates/core/src/authority.rs`의 `TrustedPeer`와 등록 PID 검증, CodeSpace `crates/runner/src/process.rs`의 프로세스 handle·PTY 소유권이다. DG-0 등록은 소비자 자격뿐 아니라 OS가 관측한 peer PID와 등록 identity의 PID가 일치해야 한다. Gateway가 worker의 PID를 대신 선언하는 방식은 이 계약에 맞지 않는다.

| 대안 | 장점 | 비용·한계 | 결정 |
| --- | --- | --- | --- |
| Runner 단일 등록 | 기존 PID 검증과 실제 실행 소유자가 일치; 등록·lease 주체 하나 | Gateway 비용까지 정적 예약에 산정하고 worker에 자격을 안전하게 전달해야 함 | 최초 채택 |
| Gateway 등록 후 worker가 같은 Principal 사용 | 시작 코드는 단순해 보임 | peer PID와 실행 소유자가 달라짐; 자격·종료 책임 불명확 | 채택하지 않음 |
| 서비스 등록 + 하위 worker 등록 | 다중 Runner·공유 서비스 예약을 표현하기 쉬움 | 부모/자식 등록·예산 이전·부분 장애·generation 계약 추가 | 실제 수요 발생 시 재검토 |

InProcess는 Gateway 내부 Runner가 같은 PID로 등록한다. UDS는 worker가 등록한다. Gateway와 Runner의 제어 비용은 하나의 정적 예약에 함께 포함한다. worker별 전체 호스트 예산이나 Gateway용 추가 전체 예산을 만들지 않는다. 명시한 최대 인스턴스 슬롯을 넘으면 거절한다.

SDK를 사용하는 CodeSpace에서 `service-exec`는 자격 FD 전달과 서비스 시작을 준비하며, 선택된 Runner가 실제 등록을 완료한다. 이는 승인 설계 §4.3의 “service-exec는 정적 제어 슬롯을 등록”이라는 일반 설명에 대한 CodeSpace 전용 구체화다. launcher와 Runner의 이중 등록을 허용하는 뜻으로 해석하지 않는다. SDK 없는 일반 서비스의 등록 방식은 별도 지원 계약이 마련될 때 정의하며 CodeSpace의 PID 검증을 완화하지 않는다.

최소 변경은 Runner의 작은 client adapter와 시작 경로, private 자격 FD 전달이다. DevGuard core에 CodeSpace process_id·PTY·workspace 정책을 넣지 않는다. `crates/pty/src/lib.rs`는 현재 상속 FD 목록에 빈 배열을 전달하지만 고정된 Codex PTY 구현은 선택 FD 상속을 지원한다. CodeSpace adapter를 확장하고 payload 이전에 자격 FD를 닫는다. 공개 MCP 인자에 FD 번호나 등록 자격을 추가하지 않는다. 이 목적만으로 Codex pin을 변경하지 않는다.

호환성 영향은 새 operator 설정과 capability, private client/Runner 시작 계약이다. 구현 비용은 중간이며 인증·FD 누출·정적 예약 과소 산정이 주요 위험이다. 정상 등록, 잘못된 UID/PID, 재사용 PID, 두 모드, FD 누출, 동일 슬롯 중복 시작을 검증한다. rollback은 신규 admission을 닫고 실행·lease를 대조한 뒤 소비 설정을 이전 검증 조합으로 돌린다. 실행 중 자격 또는 journal을 지워 회계를 초기화하지 않는다.

재검토 조건은 한 서비스가 동시에 여러 독립 Runner를 사용하거나 여러 서비스가 제어 예약을 공유해야 하는 실제 배치다. 그때 하위 등록의 생애·generation·예산 귀속을 별도 결정으로 설계한다.

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

**Required / 신뢰도 높음:** CodeSpace의 현재 spawn 후 슬롯 검사, 직렬 wire dispatch, 공유 writer, 완료 응답 중심 replay는 실제 결합 전 개선한다. 슬롯을 spawn 전에 확보하고 control/data의 대기 수·처리 수·누적 byte·보관 시간까지 제한한다. 소켓만 둘로 나누어 공유 mutex/callback이 관제를 계속 막는 구현은 완료가 아니다. 정상과 포화·느린 stdin·대량 출력·응답 유실을 시험한다. 위험은 실행/승인 중복과 수명 누수이며 CSRG-C03~C08의 PR 경계로 준비·정리·검증을 묶는다.

**Strongly Recommended / 신뢰도 높음:** CodeSpace `crates/runner/src/files.rs`의 전체 파일 read/hash 뒤 window 적용 경로는 별도 메모리 상한 개선 대상이다. 근본 원인은 응답 크기와 내부 할당량이 다른 것이다. 파일 크기 제한의 명시적 거절은 작은 해결책이고 streaming hash/read는 더 넓은 기능을 유지하는 대안이다. 소비자 API의 오류·hash 일관성을 검토해야 하며 비용은 중간이다. 큰 파일·동시 읽기·파일 변경 시험과 peak memory로 검증한다. 이 작업은 이번 runtime 범위에 추가하지 않고 후속 승인 대상으로 남긴다. 해결 전 qualification workload에서 파일 크기·동시성을 제한하고 임의 크기 파일까지 관제가 보호된다고 광고하지 않는다.

## 구현 순서와 재검토 규칙

우선 경로는 DG-0 → DG-1 → CS-RG → P1-RECOVERY다. DG-1은 독립 CLI/daemon·개발 workload·자기 적용을 검증하고, CS-RG는 결합된 Runner·승인·replay·관제 포화를 검증한다. DG-1 완료에 아직 없는 CS-RG를 요구하지 않는다. DG-LINUX는 제품 전체 완료에 필수다. DG-CACHE와 추가 adapter는 P1-RECOVERY의 선행 조건이 아니다.

위 결정이 새 증거로 변경될 때에는 영향받는 작업 ID·계약·시험·migration을 이 문서의 새 결정으로 기록한다. 기존 승인 설계, 과거 검증 증거, 완료 범위를 소급 수정하지 않는다. 이번 문서 PR은 선택을 기록하며 새 runtime API·상태 schema·dependency를 적용하지 않는다.
