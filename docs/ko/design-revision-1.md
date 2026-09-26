# 설계 개정 1: CS-RG 실행 소유권과 재사용 정책

설계 기준일: 2026-09-27. 대상: DevGuard PR #8, CodeSpace의 CS-RG 통합 계획, 두 저장소의 실행·의존성 정책. 저장소 문서 정책에 따라 [영문](../design-revision-1.md)이 정본이며, 이 문서는 사용자가 제시한 명세 원문을 정리한 검토된 한국어 대응 문서입니다.

**상태.** 2026년 9월 27일 사용자는 중간 점검 후 명시적 지시로 이 개정을 CS-RG의 새 설계 기준점으로 채택했습니다. [설계 참조](design.md), [결정 기록](planning/decisions.md), [계획 문서](planning/README.md)가 이를 반영합니다. 역사적 승인본 [design.ko.md](../design.ko.md)와 그 [checksum](../design-source.json)은 변경하지 않습니다. 이 개정은 후속 구현을 구속하는 결정을 바꾸지만 구현·qualification 상태는 바꾸지 않으며, CS-RG는 `not-started`로 남습니다. 본문에서 개별 검토 응답에 대한 언급은 중립적인 표현으로 바꾸었고, 사실 주장의 근거는 [부록](#부록-사실-주장의-근거)에 정리했습니다.

## 1. 개정의 목적과 결정 범위

이번 개정은 **초기 설계와 구현을 맞추는 문서 보정이 아니라, 사용자의 명시적 재검토 지시에 따라 현재의 구현·유지보수 조건을 반영하여 설계를 수정하는 작업**으로 정의합니다.

기존 별도 종합문이 제기한 핵심 문제는 DevGuard 통합 때문에 CodeSpace가 일반 실행과 자원 관리 실행에 대해 중복되는 PTY·프로세스 구현을 장기적으로 유지하게 될 가능성이었습니다. 이 문제의식은 유지하되, “Codex를 더 많이 사용하면 자동으로 해결된다”는 결론은 채택하지 않습니다.

**이번 개정의 최종 목표는 다음입니다.**

> **CodeSpace가 실행의 정책·상태·소유권을 공통으로 관리하고, PTY·pipe·자원 관리 여부에 따른 차이는 검증 가능한 좁은 backend 경계에 한정한다. Codex 재사용과 자체 구현은 이 목표를 달성하기 위한 수단으로 평가한다.**

2026-09-27 검토의 **D1·D2·D3 구분**을 채택하면서, 다음 사항을 추가합니다.

| 결정 | 이번 명세의 결정 |
|---|---|
| **D1 — `required` 실행 구현** | 현재 pin을 기준으로 CodeSpace가 child 수명주기를 소유하는 경로를 기본 구현안으로 삼는다. 단, “유일한 가능한 설계” 또는 “수백 줄이면 끝나는 작업”으로 표현하지 않는다. |
| **D2 — `off` 실행 통합** | 기존 동작을 보존하며 단계적으로 공통 계약으로 이동한다. **최종 CS-RG qualification 전에 실제 통합 또는 제한적 분기 유지 결정을 반드시 완료한다.** |
| **D3 — DevGuard의 Codex 의존** | 현재 구현에는 Codex 의존성을 추가하지 않는다. 다만 저장소 전체에 대한 영구 금지 논리 대신, **core·공용 client의 독립성과 실행 어댑터의 조건부 재사용 정책**으로 설계 근거를 개정한다. |
| **A4 — 출처를 명시한 코드 이식** | 무제한 허용하지 않는다. 본 명세의 범위·출처·차이 추적·검증 조건을 만족하는 **제한적 downstream adaptation**을 설계상 허용한다. |
| **Codex pin 상승** | 이번 문서 개정에서 실제 pin을 변경하지 않는다. 후보 API의 적합성 조사와 pin 변경은 구분하며, 필요한 경우 검증 결과를 근거로 별도 변경한다. |
| **`ProcessDriver`** | 기본 채택 대상이 아니다. 출력 손실·Drop·backpressure 계약을 만족하는 경우에만 선택한다. |

여기서 **현행 구현 유지 결정과 영구적인 아키텍처 금지는 구분**합니다. 기존 설계의 존재는 변경할 문서와 검증 범위를 식별하는 정보이지, 대안을 기각하는 근거가 아닙니다.

아래는 **PR 개정과 후속 구현에 적용할 명세**입니다. 현재 저장소에 이 변경을 반영하거나 PR을 병합한 상태를 뜻하지 않습니다.

## 2. 기준 소스와 PR #8의 성격 변경

### 2.1 확인된 기준

기준일에 PR #8은 열려 있었으며, head는 `92a34721d88f39a22cdde4603958d6c447c90e76`, base는 `395315d34b5d458ea1774446727f0cb14bd8a120`였습니다. 그 head의 본문은 “DG-1 구현에 CS-RG 계획을 맞춘다”는 범위였고, 기존 8개 CS-RG 작업 단위와 4개 PR 그룹을 유지한다고 설명했습니다.

CodeSpace의 확인된 `main`은 `b6e7ed22e2c730ac987297455e250cbd6e8e8b0c`입니다. 새 개정에서는 기존 `e94d214…`를 역사적 조사 기준으로 보존하되, **실제 착수 기준은 최신 확인 SHA로 다시 고정**해야 합니다.

| 구분 | 취급 |
|---|---|
| 기존 CodeSpace `e94d214…` | 초기 통합 설계의 조사 기준으로 보존 |
| CodeSpace `b6e7ed22…` | 이번 개정의 확인 기준 |
| DevGuard `395315d…` | DG-1 완료 후 통합 계획의 기준 |
| DevGuard PR #8 `92a34721…` | 수정 대상 문서 head |
| Codex `6b9826e…` | 현재 CodeSpace 배포 pin |
| Codex `b334d5b…` 등 | 앞선 검토에서 확인한 **비교 스냅샷**. 배포 pin이나 자동 채택 대상으로 취급하지 않음 |

### 2.2 PR 설명 변경

권장 PR 제목은 다음과 같습니다.

```text
docs(architecture): revise CS-RG execution ownership and reuse policy
```

PR의 목적 설명은 아래 의미를 포함하도록 다시 작성합니다.

> 이 PR은 사용자의 명시적인 설계 재검토 지시에 따라 CS-RG 실행 계층을 개정한다. DG-1 구현과의 정합성뿐 아니라 CodeSpace의 중복 수명주기, spawn 보호, 출력 계약, backend 유지보수 비용을 함께 다룬다. 기존 설계의 권한·자원 정확성 목표는 유지하되, 그 목표를 구현하는 경계와 작업 순서는 재평가한다.

기존 본문의 다음 표현은 변경해야 합니다.

| 기존 표현 | 개정 방향 |
|---|---|
| “구조는 바뀌지 않는다” | 실행 계층과 검증 순서를 개정하며, 필요하면 작업 단위·그룹도 변경한다고 명시 |
| “Codex 어댑터는 `off`에 남긴다” | 초기 호환성 유지 조치이며, 최종 qualification 전 유지·통합 판단 대상이라고 명시 |
| “pin은 유지한다” | 이번 변경 범위의 결정으로 표현. 향후 변경 금지로 해석되지 않게 함 |
| “사용자 결정이 있으면 설계를 바꾼다” | 이번 지시를 재검토·설계 개정의 근거로 기록 |
| “문서만 변경한다” | **이 PR의 파일 변경은 문서·문서 검증에 한정하지만, 후속 구현을 구속하는 설계 결정은 변경한다**고 구체화 |

DevGuard의 현재 `AGENTS.md`도 사용자의 현재 지시가 우선한다고 명시합니다. 따라서 역사적 승인본을 보존하면서 편집 기준 설계를 개정하는 방식이 적합합니다.

## 3. 반드시 유지할 목표와 변경 가능한 구현 선택

개정 문서에서는 다음 둘을 같은 수준의 제약으로 취급하지 않아야 합니다.

### 3.1 유지할 정확성·보안 목표

**실행당 하나의 명확한 소유자, 중복 실행 방지, 자격 증명 보호, 보수적인 자원 반환, 기존 권한 모델 보존**은 유지합니다.

구체적으로는 다음 조건을 불변식으로 둡니다.

| ID | 불변식 |
|---|---|
| INV-01 | 프로세스마다 실제 회수 책임자는 하나여야 한다. |
| INV-02 | DevGuard 관리 실행은 필요한 회수 전 관찰 기회를 잃지 않아야 한다. |
| INV-03 | permit·credential·transcript FD가 관계없는 실행이나 사용자 payload에 잘못 전달되어서는 안 된다. |
| INV-04 | 응답 유실·timeout·EOF·root reap만으로 실행 미발생이나 전체 scope 종료를 확정하지 않는다. |
| INV-05 | 준비된 실행은 일회성으로 소비하며, 불확실한 실행을 자동 재실행하지 않는다. |
| INV-06 | 신규 자원 허가 실패가 기존 프로세스의 조회·종료 요청 경로를 막지 않는다. |
| INV-07 | 프로세스 종료, 출력 정리, workspace 점유 해제, resource lease 반환을 구분한다. |
| INV-08 | 구현 완료, 기능 검증, 플랫폼 검증, SLO qualification을 별도로 기록한다. |

이 원칙들은 현재 DevGuard [계약](contracts.md)과 [통합 계획](planning/codespace-integration.md)의 핵심이기도 합니다.

### 3.2 재평가할 구현 선택

반면 다음은 목적이 아니라 **변경 가능한 수단**입니다.

- `off`와 `required`가 서로 다른 spawn 함수를 호출하는 구조.
- Codex 고수준 `spawn_pty_process()`의 지속 사용 여부.
- CodeSpace 내부 PTY opener의 구현 출처.
- DevGuard 저장소 전체의 Codex 의존 금지.
- 기존 Codex pin.
- 기존 8개 작업·4개 PR 그룹이라는 계획 수량.

**정확성 목표는 유지하되, 구현 선택은 이번 최적화 범위에서 다시 결정합니다.**

## 4. PR #8의 F1 개정 명세

기존 F1은 pinned Codex spawn의 내부 reap과 FD 전달 문제를 근거로 Runner-owned 실행을 제안합니다. 개정 후에는 이를 아래 다섯 하위 항목으로 나눕니다.

| 하위 항목 | 문제 | 필요한 변경 |
|---|---|---|
| **F1a — 회수 소유권** | waiter 외의 종료·timeout·Drop 경로도 child를 회수할 수 있음 | 모든 회수 경로를 조사하고 실행당 하나의 책임자로 통합 |
| **F1b — FD 전달** | “FD를 닫지 않음”과 “CLOEXEC FD를 특정 자식에만 전달”은 다름 | 준비부터 payload까지 FD 소유·상속·종료 규칙 명세 |
| **F1c — 동시 spawn** | 다른 스레드의 spawn이 private FD 생성 구간과 경쟁할 수 있음 | 프로세스 전체의 spawn 보호 규칙과 예외 증명 |
| **F1d — 출력·핸들 계약** | bridge 내부의 손실과 Drop 동작이 상위 계약과 다를 수 있음 | 출력 손실·backpressure·Drop 적합성 검사 |
| **F1e — 유지보수 분기** | 공통 인터페이스만 만들고 실제 중복 구현을 영구 유지할 가능성 | CS-RG 종료 전 backend 통합 또는 제한적 유지 결정 |

### F1의 결론 문구

다음 의미로 교체하는 것이 적절합니다.

> 현재 고정된 Codex 고수준 PTY spawn은 변경 없이 DG-1 관리 실행에 사용할 수 없다. 원인은 회수 소유권과 FD 전달 계약의 불일치이며, 이를 단순 wrapper로 숨겨서는 안 된다.\
> CodeSpace는 공통 실행 계약과 단일 수명주기 조정 계층을 도입한다. `required`는 회수 전 관찰이 가능한 backend를 사용한다. 기존 `off` backend는 초기 호환성을 위해 유지할 수 있지만, 최종 CS-RG qualification 전에 통합 또는 제한적 분기 유지 근거를 확정한다.

**“Codex가 불가능하다”가 아니라 “확인한 API를 변경 없이 사용할 수 없다”로 범위를 제한**해야 합니다.

## 5. CodeSpace 목표 아키텍처

### 5.1 공통화의 중심은 PTY 함수가 아니라 실행 계약

목표 구조는 다음과 같습니다. 아래 이름은 **새 설계의 개념 이름이며 현재 존재하는 API를 뜻하지 않습니다.**

```text
CodeSpace Gateway
    │
    │ authorization / workspace / approval
    ▼
Runner ExecutionCoordinator
    │
    ├─ ResourceGovernor
    │    ├─ OffGovernor
    │    └─ DevGuardGovernor
    │
    ├─ PreparedExecution / LaunchPlan
    │
    ├─ ProcessSupervisor
    │    ├─ status / timeout / termination
    │    ├─ exit observation / reap coordination
    │    └─ output / retention / release coordination
    │
    └─ ProcessBackend
         ├─ LegacyCodexPty
         ├─ LegacyTokioPipe
         └─ OwnedUnixProcess
              ├─ PipeTransport
              └─ PtyTransport
```

공통으로 유지할 부분은 **실행 identity, 승인과 실행의 연결, 상태 전이, timeout, 종료 요청, 출력 기록, 완료 및 반환 조정**입니다.

backend에 남길 부분은 **OS child 생성, 입출력 연결, terminal 설정, 플랫폼별 종료 관찰 수단, 실제 reap 수단**입니다.

#### 중요한 제한

공통 `ProcessSupervisor`를 만든다고 legacy Codex backend의 실제 waiter까지 Runner가 즉시 소유하는 것은 아닙니다.

따라서 다음 두 소유 모델을 명시적으로 구분합니다.

| 모델 | 의미 | 사용 가능 범위 |
|---|---|---|
| `BackendReaped` | backend가 실제 reap을 수행하고 결과를 보고 | 초기 legacy `off` 경로 |
| `OwnerControlledReap` | CodeSpace가 비회수 관찰과 실제 reap 순서를 제어 | DevGuard `required` 경로 |

**`BackendReaped` 경로에 가상의 `ExitedUnreaped` 상태를 만들어서는 안 됩니다.** 실제로 보장하지 못하는 capability를 공통 인터페이스가 만들어 낸 것처럼 광고하지 않아야 합니다.

### 5.2 모듈 책임

불필요하게 새 daemon이나 독립 저장소를 만들지 않고, CodeSpace 내부 모듈 또는 좁은 crate로 구성합니다.

| 제안 위치 | 책임 |
|---|---|
| `crates/runner/src/execution/` | 실행 조정, 계획 소비, 상태 전이 |
| `crates/runner/src/process/` | supervisor와 backend 연결 |
| `crates/resource-client/` | DevGuard client·오류·capability 변환 |
| 기존 `crates/pty/` | Codex 타입 격리 및 legacy PTY 경로 |
| CodeSpace 내부 Unix transport 모듈 | 필요한 PTY allocation·stdio·resize 구현 |

이는 확정된 디렉터리 이름보다 **책임 경계**가 우선입니다. 구현 과정에서 모듈 구성을 조정할 수 있지만, 아래 조건은 유지해야 합니다.

**DevGuard client 타입과 Codex 타입이 CodeSpace의 공개 MCP 타입으로 새어 나오지 않아야 합니다.** 또한 generic 실행 조정 계층이 두 upstream의 내부 타입을 직접 혼합하지 않도록 합니다.

## 6. `PreparedExecution`과 `LaunchPlan` 계약

### 6.1 알림용 hook이 아니라 일회성 실행 준비 객체

DevGuard는 실행 전후에 통지만 받는 관찰자가 아닙니다. helper로 실행 대상을 구성하고, permit과 transcript를 전달하며, 생성 경로를 제한합니다.

실제 `HelperCommand`도 설정용 표준 `Command`를 제공하면서, 생성은 자신의 `spawn()`을 통해 수행하도록 요구합니다.

따라서 `prepare()`는 다음을 포함하는 **소유권 있는 실행 준비 결과**를 반환해야 합니다.

| 구성 | 계약 |
|---|---|
| 실행 identity | CodeSpace process ID와 DevGuard attempt의 고정된 연결 |
| 명령 의미 | executable, argv, cwd, 환경 변경, tty, timeout, workspace·policy 식별 |
| 실행 슬롯 | 실제 spawn 전에 확보 |
| workspace 점유 | 기존 FIFO 및 승인 흐름과 연결 |
| 자원 준비 | `off` 또는 DevGuard Prepared 상태 |
| 만료 | 최초 준비 시점의 deadline을 유지 |
| 소비권 | 한 번만 실행하거나 취소 가능 |
| 실패 정리 | 미실행 정리와 committed/uncertain 정리를 구분 |

개념적으로 다음과 같이 구성할 수 있습니다.

```text
PreparedExecution
    identity
    meaning_digest
    slot_guard
    workspace_guard
    original_deadline
    launch_plan
    resource_state

LaunchPlan
    Plain(...)
    GovernedHelper(...)
```

`GovernedHelper` 안의 실제 DevGuard 객체는 어댑터 내부에 격리할 수 있습니다. 중요한 것은 타입 이름이 아니라 **일회성 소비와 안전한 정리 책임**입니다.

#### 금지할 구현

준비 결과를 자유롭게 `Clone`하여 두 곳에서 spawn하거나, serialization한 argv만으로 준비 객체를 다시 생성해서는 안 됩니다.

또한 **취소된 async task의 Drop에서 resource lease가 자동 반환됐다고 가정하지 않아야 합니다.** 비동기 원격 정리가 필요한 상태는 명시적인 reconciliation 작업으로 남겨야 합니다.

### 6.2 실행 순서

기존 승인·workspace 정책을 유지하며 다음 순서로 처리합니다.

```text
권한·명령·workspace 정책 검사
    ↓
workspace FIFO 획득
    ↓
Runner 실행 슬롯 확보
    ↓
자원 준비: Admit
    ↓
동일 attempt의 approval CAS
    ↓
BeginLaunch
    ↓
LaunchPlan을 한 번 소비하여 helper 생성
    ↓
helper transcript와 process handle을 각각 추적
    ↓
프로세스 종료 관찰
    ↓
필요한 회수 전 Observe
    ↓
reap
    ↓
출력·workspace·resource lease를 각각 정리
```

현재 계획의 **준비 예산 250ms와 Prepared 수명 5초**는 서로 다른 제한입니다. DevGuard의 프레임별 250ms 제한 역시 별도입니다. 새 구현에서는 이 세 값을 같은 timeout으로 취급하지 않아야 합니다.

특히 프레임마다 250ms를 허용한다고 전체 준비가 250ms 안에 끝나는 것은 아닙니다. C00 검증에서는 연결·인증·등록·Admit까지의 end-to-end deadline 전파를 확인해야 합니다.

## 7. 프로세스 수명주기와 회수 계약

### 7.1 상태를 하나의 `finished`로 합치지 않음

최소한 다음 네 축을 독립적으로 관리합니다.

| 축 | 예시 상태 |
|---|---|
| 실행 준비·dispatch | Prepared, Committed, HelperSpawned, Uncertain |
| OS child | Running, ExitedUnreaped, Reaped, OwnershipLost |
| 입출력 | Open, EOF, Truncated, Failed |
| DevGuard resource | Reserved, Active, Suspect, Released |

이를 통해 다음 오해를 구조적으로 방지합니다.

```text
stdout EOF ≠ process exit
process exit ≠ reap
reap ≠ descendant 종료
출력 보존 만료 ≠ resource lease 반환
helper READY ≠ payload 실행 성공
```

현재 DevGuard의 `LaunchOutcome::Started`도 transcript에서 관찰한 실행 징후이지, helper가 READY 직후 종료되는 모든 경우를 배제하는 강한 실행 성공 증명은 아닙니다. 통합층에서 이 이름을 더 강한 의미로 바꾸지 않아야 합니다.

### 7.2 단일 회수 책임자

`required` 경로에서는 다음 규칙을 적용합니다.

**실제 child를 소유하는 객체 외부에서는 `wait`, `try_wait`, `waitpid` 등 회수 가능한 연산을 호출하지 않습니다.**

종료 요청과 timeout 처리는 child를 직접 기다리지 않고 supervisor에 의도를 전달합니다.

```text
terminate_process ─┐
timeout ───────────┼─→ supervisor → 검증된 종료 동작
shutdown ──────────┘

OS exit notification
    → supervisor의 비회수 관찰
    → Observe
    → 동일 소유 객체로 reap
```

현재 CodeSpace는 일반 waiter 외에도 종료 요청 경로에서 `try_wait()`를 호출합니다. 따라서 waiter 함수 하나를 교체하는 수준으로는 충분하지 않습니다.

#### 반드시 조사할 호출 지점

`spawn_pipe`, 종료 감시, timeout, `request_kill`, workspace 단위 종료, shutdown, backend Drop, task 취소, 오류 정리까지 포함합니다.

grep으로 발견한 위치만 목록화하지 말고, **최종적으로 누가 child를 소유하고 각 경로가 어떤 메시지를 보내는지**를 소유권 표로 남겨야 합니다.

### 7.3 회수 전 관찰의 실패 처리

기본 흐름은 현재 DevGuard CLI가 사용하는 방식과 맞춥니다.

```text
종료를 확인하되 회수하지 않음
    ↓
회수 전 Observe 시도
    ↓
성공·실패·timeout을 기록
    ↓
소유 중인 Child를 통해 회수
    ↓
필요한 후속 Observe / reconciliation
```

현재 CLI도 회수 전 관찰과 회수 후 관찰을 구분하며, 회수 전 관찰 실패를 기록한 뒤 child를 회수합니다.

이번 설계에는 다음 제한을 추가합니다.

- 회수 전 관찰은 전체 deadline을 가져야 합니다.
- 실패했다고 resource lease를 반환하지 않습니다.
- 관찰 성공을 기다리며 zombie를 무기한 유지하지 않습니다.
- 원래 child 소유 객체를 남긴 상태에서 다른 코드가 직접 reap하지 않습니다.
- `ECHILD`나 소유권 상실은 성공적인 관찰 또는 미실행 증거가 아닙니다.

**새 정책 제안:** 회수 전 Observe의 전체 시도 상한은 우선 **1초**로 두고 C00에서 검증합니다. 이는 기존에 qualification된 값이 아니라 이번 개정에서 제안하는 cleanup 예산입니다. 이 예산이 상태 조회나 종료 요청의 응답을 막아서는 안 됩니다.

이를 구현할 수 없는 blocking 호출을 단순히 timeout future로 감싸고 “취소됐다”고 취급해서는 안 됩니다. 실제 호출이 계속 실행된다면 소유권과 정리 책임도 계속 추적해야 합니다.

### 7.4 관찰과 signal의 동시성

종료 요청은 DevGuard에 새 자원 허가를 받지 않고 처리할 수 있어야 합니다. 그러나 이것이 **authority 장애 중에도 전체 scope 종료를 무조건 증명할 수 있다**는 뜻은 아닙니다.

Runner가 안전하게 식별할 수 있는 대상에 대해 기존 제어권을 행사하되, 자손이나 scope 전체의 종료를 확인할 수 없으면 결과를 불완전하게 표시하고 자원을 반환하지 않습니다.

새 managed backend에서 오래된 숫자 PID·PGID만 보관한 채 무조건 signal을 보내는 방식은 금지합니다. 특히 Codex의 process-group 종료 코드를 가져올 때도 DevGuard의 identity·scope 계약과 별도로 대조해야 합니다.

## 8. spawn 보호와 FD 계약

### 8.1 단일 관문은 단일 전역 lock wrapper가 아님

현재 `HelperCommand::spawn()`은 내부에서 이미 `spawn_guard()`를 획득합니다. 따라서 외부 공통 관문이 같은 guard를 잡은 채 다시 호출하면 안 됩니다.

Rust의 표준 `Mutex`는 같은 스레드가 중복 획득했을 때 정상 반환을 보장하지 않으므로, 재진입 가능한 lock처럼 취급해서는 안 됩니다.

공통 관문이 보장해야 하는 것은 다음입니다.

> **모든 spawn이 동일한 FD 보호 규칙을 따르며, 각 경로에서 보호 책임자가 정확히 한 번 지정된다.**

| 실행 경로 | 보호 책임 |
|---|---|
| DevGuard `HelperCommand` | helper API 내부 보호를 사용. 외부 중복 획득 금지 |
| 직접 생성하는 pipe·helper | 실제 spawn 구간에서 공통 guard 또는 검증된 동등 보호 사용 |
| legacy Codex PTY | 실제 spawn·FD 정리 경로를 조사하여 안전한 참여 방식 입증 |
| 커널 수준 close-on-exec-default 경로 | 적용·오류·fallback 경로까지 검증한 경우에만 예외 인정 |

**자기 FD를 CLOEXEC로 만든다는 사실만으로, 다른 스레드가 FD를 생성하는 위험 구간과의 경쟁까지 해결되는 것은 아닙니다.**

또한 별도 버전의 client crate가 같은 프로세스에 중복 링크되어 서로 다른 static guard를 갖는 경우도 검증 대상입니다. “같은 이름의 mutex를 사용한다”와 “동일한 보호 객체를 공유한다”를 구분해야 합니다.

### 8.2 보호 범위

조사 범위는 `exec_command`만이 아닙니다.

같은 OS 프로세스 안에서 실행하는 patch helper, sandbox helper, 보조 명령, worker 생성, 테스트 helper 등 **실제 child 생성 경로 전체**를 포함합니다.

Gateway와 UDS Runner는 서로 다른 프로세스이므로, 각각의 FD 생성·spawn 보호 범위를 따로 조사합니다. 전역 daemon lock으로 해결되는 문제가 아닙니다.

### 8.3 임계 구역 제한

spawn guard를 잡은 상태에서 다음 작업을 수행하지 않습니다.

```text
DevGuard 네트워크 요청
permit 승인 대기
transcript 전체 수신
child 종료 대기
출력 drain
장시간 로그·파일 기록
```

guard는 필요한 FD 생성·상속 설정·실제 child 생성 경계를 보호해야 합니다.

외부 API가 async 함수라는 이유만으로 전체 future를 표준 mutex 아래 실행하지 않습니다. 실제로 어느 시점에 child를 생성하고 어떤 대기를 수행하는지 확인해야 합니다.

그 안전성을 확인하지 못하면 **legacy 경로와 managed 경로의 혼용을 검증 완료로 선언할 수 없습니다.** 같은 범위에서 backend를 교체하거나, 지원 조합을 명시적으로 제한해야 합니다.

### 8.4 PTY 설정과 helper FD의 합성

PTY setup을 추가할 때는 기존 `HelperCommand`의 permit·transcript 전달 동작을 보존해야 합니다.

필수 조건은 다음과 같습니다.

| 항목 | 요구사항 |
|---|---|
| 부모의 private FD | helper 생성 전까지 보호된 상태로 유지 |
| 자식의 필수 FD | permit·transcript·정당한 jobserver FD를 구분하여 전달 |
| payload 경계 | credential FD는 닫고 transcript는 exec 시 닫히도록 유지 |
| stdio | PTY slave를 stdin·stdout·stderr에 연결 |
| terminal | session·controlling terminal 설정 순서를 명시 |
| failure | setup 실패 시 child·master/slave·private FD를 모두 정리 |
| 재실행 | helper의 QoS 재실행을 거쳐도 필요한 FD와 PID 의미 유지 |

`pre_exec`는 일반적인 Rust 실행 환경이 아닙니다. 추가 callback에서 allocation, mutex 획득, 환경 조회 등 안전하지 않은 작업을 수행하지 않도록 사전에 데이터를 준비해야 합니다. 등록된 callback의 실행 순서도 고려해야 합니다.

특히 **helper가 필요로 하는 FD를 보존 목록에서 빠뜨리는 cleanup**, **PTY 설정과 process-group 설정을 무작정 중복 적용하는 코드**를 금지합니다.

## 9. 출력·backpressure·`ProcessDriver` 선택 기준

### 9.1 기본 핸들은 CodeSpace 소유

CodeSpace의 public process handle과 출력 기록은 CodeSpace가 소유합니다. Codex `ProcessHandle`을 재사용하는 것 자체를 목표로 삼지 않습니다.

현재 pinned `ProcessDriver` bridge에는 broadcast 지연으로 빠진 항목을 건너뛰는 처리가 있고, `ProcessHandle` Drop은 종료 및 I/O 정리를 수행합니다.

따라서 `ProcessDriver`는 다음 조건을 모두 만족할 때만 채택합니다.

| 조건 | 합격 기준 |
|---|---|
| 출력 손실 | bridge 내부에서 사라진 출력이 상위 기록에 숨겨지지 않음 |
| backpressure | 느린 reader가 종료·상태 요청을 막지 않음 |
| byte 제한 | 채널 개수 제한뿐 아니라 전체 byte 상한이 정의됨 |
| Drop | UI·응답 객체·임시 handle의 Drop이 잘못된 종료를 유발하지 않음 |
| 종료 callback | 중복 종료·회수 경쟁·stale identity 사용이 없음 |
| output drain | exit 통지와 최종 출력의 순서 차이를 처리 |
| 비용 | 추가 변환·버퍼·task가 제거되는 코드보다 유리함 |

특히 broadcast의 `Lagged(n)`은 곧바로 정확한 손실 byte 수가 아닙니다. 정확한 byte accounting이 필요한 경우 sequence·length metadata나 별도 손실 경로가 있어야 합니다.

**이 조건을 충족하기 위해 Codex handle을 다시 복잡하게 감싸야 한다면, CodeSpace의 기존 출력·핸들 추상화를 직접 사용하는 쪽을 선택합니다.**

### 9.2 공통 출력 계약

출력은 가능한 한 하나의 CodeSpace 기록 계층으로 수렴시킵니다.

```text
backend output
    ↓
CodeSpace output collector
    ↓
bounded retention
    ↓
read_process / output_lost
```

관계없는 실행의 출력을 섞지 않고, EOF와 종료 상태를 구분하며, 의도적인 retention 손실과 transport 내부의 알 수 없는 손실을 구분합니다.

정확한 손실량을 알 수 없는데 `output_lost=false`로 반환하는 방식은 허용하지 않습니다. 기존 프로토콜로 이 상태를 표현할 수 없다면, 필요한 호환성 변경을 CSRG-C06에서 명시적으로 설계합니다.

## 10. D1·D2·D3의 구체적인 선택과 종료 조건

### 10.1 D1 — managed PTY

**기본안은 A1: 현재 pin에서 가능한 CodeSpace-owned Unix transport입니다.**

다만 그 범위를 “PTY opener 몇 줄”로 축소하지 않습니다. 실제 유지 대상에는 master/slave 수명, signal·session 설정, resize, FD 실패 정리, 출력 처리, 취소·shutdown 연계가 포함됩니다.

Codex 코드가 유용한 경우 아래 순서로 평가합니다.

```text
기존 공개 API
    ↓
같은 계약을 제공하는 upstream 후보
    ↓
제한적 downstream adaptation
    ↓
필요한 범위의 자체 구현
```

이는 반드시 위 순서대로 모든 구현을 시도하라는 뜻이 아니라, **새 코드를 작성하기 전에 재사용 가능성과 계약 차이를 기록하라는 요구**입니다.

#### A4 허용 범위

이번 설계 개정에서는 아래 조건을 만족하는 출처 명시 이식을 허용합니다.

**허용 대상:** PTY allocation, terminal setup, resize, 제한된 I/O 보조 코드 등 명확히 분리된 실행 메커니즘.

**허용하지 않는 대상:** `codex-core` 제품 의미, 세션 권한, Agent Loop, 광범위한 crate 복사, 의존성 검사를 회피하기 위한 코드 복제.

각 adaptation에는 다음 기록이 필요합니다.

```text
원본 저장소·full SHA·파일 경로
가져온 범위
변경 이유
의도적으로 달라진 동작
대응 테스트
upstream 갱신 시 재검토 조건
제거·upstream 재수렴 조건
```

기존 CodeSpace 정책은 호환되지 않는 의존성을 숨기기 위한 upstream crate 복사를 금지합니다. 이 금지는 유지하고, **감사 가능한 제한적 adaptation을 별도 정책으로 정의**해야 합니다.

### 10.2 D2 — 기존 `off` 경로

D2는 더 이상 “나중에 여유가 생기면 검토”하는 선택 작업으로 남기지 않습니다.

**CS-RG 최종 qualification 전에 다음 둘 중 하나를 결정해야 합니다.**

#### 결과 A: 실제 backend 통합

동작 동등성과 유지보수 이익이 확인되면 해당 플랫폼·transport의 기존 경로를 제거합니다.

이 경우 문서만 공통화했다고 끝내지 않고, 사용하지 않는 코드·분기·테스트 fixture·의존성을 실제로 정리합니다.

#### 결과 B: 제한된 compatibility backend 유지

통합이 현재 손해라면 기존 backend를 남길 수 있습니다. 그러나 다음 정보가 필요합니다.

| 항목 | 요구 |
|---|---|
| 남기는 이유 | 구체적인 동작 차이 또는 비용 근거 |
| 남는 범위 | 어떤 플랫폼·transport·mode인지 |
| 공통화된 부분 | 상태, timeout, 오류, 출력 등 |
| 중복된 부분 | 실제 별도 유지해야 하는 코드와 테스트 |
| 지원 계약 | 해당 backend가 제공하지 않는 capability |
| 재검토 시점 | 다음 관련 upstream 변경·확장·결함 수정 시점 |
| 제거 기준 | 무엇이 충족되면 통합할 것인지 |

**“기존 코드이므로 유지한다” 또는 “parity가 통과했으므로 반드시 교체한다” 모두 충분한 결정 근거가 아닙니다.**

다만 분기를 남기는 쪽도 유지보수 비용을 입증해야 합니다. 평가 부담을 새 구현에만 부과해서는 안 됩니다.

### 10.3 D3 — DevGuard 의존성 정책

현재 구현에 Codex 의존성을 추가하지 않는 C0을 선택하되, 편집 기준 설계의 논리를 다음처럼 바꿉니다.

#### 유지할 경계

```text
devguard-contract
devguard-core
범용 client 계약
```

위 계층에는 Codex 제품 타입, 모델 세션, CodeSpace workspace 권한, PTY 소유권을 넣지 않습니다.

#### 개정할 경계

“DevGuard 저장소 전체는 어떤 상황에서도 Codex를 사용하지 않는다”는 영구 금지 대신 다음을 명시합니다.

> DevGuard의 기본 배포와 공용 client는 현재 Codex에 의존하지 않는다. 실행·플랫폼 어댑터의 저수준 유틸리티 재사용은 실제 대체 코드, 계약 적합성, 의존성 전파, 복구 경로 및 재검증 비용을 평가하여 결정한다.

현재 `HelperCommand`·launcher·native observation 중 어느 부분을 실제로 대체하는지 보여 주지 못하면 새 의존성을 추가하지 않습니다.

#### 재검토 트리거

2026-09-27 검토의 트리거를 채택하되, 단순한 기능 이름 등장으로 자동 채택하지 않습니다.

| 트리거 | 다시 확인할 내용 |
|---|---|
| DG-LINUX 착수 | cgroup 배치 시점, helper layering, 부모·자식 관계, fork 안전성 |
| 공개된 범용 FD attachment·외부 회수 소유 API | 실제 fallback·Drop·취소 경로까지 계약 충족 여부 |
| DevGuard의 child 감독 범위 확대 | 실제 중복되는 코드가 생겼는지 |
| 반복되는 동일 OS 결함 수정 | 공통 구현이 수정·검증 비용을 줄이는지 |

그 검토의 **“전이 의존성 36개”**는 해당 검토에서 얻은 수치로만 기록합니다. 채택 기준으로 사용하려면 SHA, target, feature, runtime/build/dev 구분과 실행 명령이 함께 있어야 하며, 개정 문서에서 보편적인 고정 비용처럼 사용하지 않습니다.

## 11. 실패·승인·자원 반환 매핑

기존 PR #8의 표는 permit 응답 유실, helper 미생성, helper의 READY 전 종료를 가깝게 묶고 있습니다. 개정 시에는 **실행 미발생 증거와 자원 정리 증거를 분리**해야 합니다.

| 상황 | 실행 판단 | 자원·승인 처리 |
|---|---|---|
| Admit 명시적 거절 | 사용자 코드 미실행 | approval 미소비, 준비 guard 정리 |
| Admit 응답 유실 | child는 아직 만들지 않았어도 예약 상태는 불명 | 같은 attempt 조회·취소. 원격 반환 확인 전 임의 반환 금지 |
| BeginLaunch 응답 유실 | permit을 받지 못했는지, 실행 경로에 넘겼는지 로컬 소유권 확인 필요 | 동일 attempt 유지. 안전한 경우만 AbandonLaunch |
| helper spawn의 확정 실패 | 생성 API가 보장하는 실패 범위 사용 | 미생성 증거를 authority에 전달하고 확인 |
| 유효한 `failed`·`refused` transcript | helper의 미실행 보고 | claimed scope와 unclaimed grant를 구분하여 정리 |
| READY 이후 `exec_failed` | 실행 준비를 통과한 뒤 executable 진입 실패 | 단순 budget 거절과 구분. 자동 approval 재사용 금지 |
| READY 후 transcript 유실 | 실제 실행을 배제할 수 없음 | unknown 유지, 자동 replay 금지 |
| root 종료, 자손 생존 | root 종료만 확인 | scope·workspace·resource 정리를 따로 판단 |
| Observe 실패 또는 tracking loss | 전체 종료를 증명하지 못함 | reap은 제한된 절차로 수행하되 resource는 보수적으로 유지 |

**`AbandonLaunch` 호출 자체를 `NoHelperCreated` 증거로 취급하지 않습니다.** authority가 무엇을 확인하여 어떤 상태를 반환했는지가 중요합니다.

또한 user executable이 125·126·127을 반환할 수도 있으므로, exit code만으로 helper의 실행 단계나 승인 재사용 가능성을 추론하지 않습니다.

## 12. 작업 단위와 PR 순서 개정

기존 CS-RG 계획은 C01–C08, P1–P4로 구성됩니다. 이번 개정에서는 **최소 적합성 검증과 D2의 종료 결정을 별도 필수 작업으로 추가**하는 것을 권고합니다. 기존 개수를 유지하기 위해 이를 C03 안에 숨기지 않습니다.

### 12.1 개정 작업 구성

아래 ID와 그룹은 **이번 명세의 제안값**입니다.

| 그룹 | 작업 | 핵심 산출물 |
|---|---|---|
| **CSRG-P0** | **C00 — 실행 경계 적합성 검증** | spawn·reap 호출 목록, 최소 managed PTY 검증, FD 보호와 cleanup 적합성 보고 |
| **CSRG-P1** | C01·C02 | client pin, helper provenance, consumer provisioning, 등록·설정 |
| **CSRG-P2** | C03·C04 | 공통 supervisor, PreparedExecution/LaunchPlan, managed 실행·승인 일관성 |
| **CSRG-P3** | C05·C06 | control/data 보호, 출력·replay·lifecycle 전체 상한 |
| **CSRG-P4** | C07·**C09 — backend 유지보수 수렴 결정** | parity·fault 검증, `off` 통합 또는 제한적 유지 결정과 코드 정리 |
| **CSRG-P5** | C08 | 최종 artifact 조합의 CodeSpace 통합 qualification |

이 안을 채택하면 CS-RG는 **10개 작업 단위·6개 그룹**이 됩니다. 기존 전체 수량 46/23을 사용하는 문서에서는 다른 작업 변경이 없다는 조건으로 **48/25**가 되며, 실제 계획 데이터에서 재계산하여 반영해야 합니다.

#### C00의 범위

C00은 별도의 완성형 프로세스 프레임워크를 만드는 작업이 아닙니다.

**managed helper를 PTY에 연결하고, FD가 안전하며, root 종료 후 관찰·회수 순서를 소유할 수 있음을 최소 구현으로 검증**합니다.

검증 코드는 후속 공통 테스트로 흡수하거나 삭제해야 합니다. C00용 backend를 제품에 별도로 남겨 네 번째 경로를 만드는 것은 금지합니다.

#### C09의 범위

C09는 문서상 “검토했다”로 완료할 수 없습니다.

통합을 선택했다면 실제 불필요 코드와 의존성을 제거하고, 분기 유지를 선택했다면 **범위·비용·테스트·재검토 조건이 있는 결정 기록**을 제출해야 합니다.

C09에서 코드가 바뀌면 C07의 관련 parity를 다시 수행하고, **그 최종 head를 C08에서 측정**합니다.

### 12.2 기존 작업 단위의 수정

| 작업 | 추가·수정할 내용 |
|---|---|
| C01 | client와 helper 출처 분리, 실제 runtime/build 의존성 검사, 전이 Codex 의존 유입 검사 |
| C02 | instance와 session 구분, 자원 설정 검증, 동일 프로세스의 mixed-mode 보호 규칙 |
| C03 | 슬롯 선점뿐 아니라 모든 회수 경로 통합, backend capability, cleanup 책임 포함 |
| C04 | 일회성 LaunchPlan 소비, BeginLaunch 유실, transcript 단계별 승인 처리 |
| C05 | spawn·Observe 지연이 control path로 전파되지 않는지 검증 |
| C06 | bridge 손실 포함 출력 accounting, 총 byte 상한, retention과 lease 분리 |
| C07 | `off`/`required`·pipe/PTY·InProcess/UDS 및 동일 프로세스 혼용 검증 |
| C08 | C09 후 최종 구현·pin·artifact 조합만 qualification 대상으로 채택 |

**기존 C03에 “작은 opener 추가”라는 표현은 사용하지 않습니다.** 실행 소유권 개정은 주요 작업이며, 코드량보다 경쟁·실패 경로의 검증 범위로 평가해야 합니다.

## 13. 검증 및 완료 조건

### 13.1 필수 검증 행렬

기본 행렬은 다음과 같습니다.

```text
resource mode: off / required
transport:     pipe / PTY
runner mode:   InProcess / UDS
```

여기에 **같은 프로세스 안에서의 혼합 실행**을 별도 축으로 추가합니다. 각 경로의 단독 성공만으로 FD·spawn 보호가 검증되는 것은 아닙니다.

| 검증 영역 | 필수 사례 |
|---|---|
| 슬롯·준비 | 동시 한도 초과, 준비 만료, 취소, 늦은 worker 실행 |
| 실행 identity | 동일 attempt 재요청, 의미 변경 충돌, 응답 유실 |
| helper | permit 유실·잘못된 helper·READY 전후 실패 |
| 회수 | root 즉시 종료, 생존 자손, timeout·terminate·shutdown 경쟁 |
| 소유권 | waiter 취소, backend Drop, `ECHILD`, 이중 reap 방지 |
| FD | 관계없는 동시 spawn, payload 검사, jobserver 유지, 실패 정리 |
| PTY | 초기 크기, resize, controlling terminal, session·group, EOF |
| 출력 | 큰 출력, 느린 reader, bridge lag, tail 보존, 상한 초과 |
| authority 장애 | pre-reap Observe 장애, daemon 재시작, 신규 허가 실패 중 기존 제어 |
| rollback | 신규 시작 차단, 기존 실행 drain, unknown 보존, stale journal 복원 금지 |

#### 안전한 실패의 합격 기준

실패 주입 테스트에서 모든 resource가 즉시 반환되어야 하는 것은 아닙니다.

**불확실성이 남으면 Suspect 또는 charged 상태가 유지되는 것이 올바른 결과일 수 있습니다.** 대신 이 상태가 관찰 가능하고, 허위 성공·자동 재실행·잘못된 자원 재할당으로 이어지지 않아야 합니다.

### 13.2 유지보수 최적화의 측정 항목

“코드가 짧아졌다”만으로 D2를 결정하지 않습니다.

| 측정 항목 | 확인 목적 |
|---|---|
| 제품 내 spawn 진입점 수 | 우회 경로가 줄었는가 |
| 실제 reap 수행 지점 수 | 소유권이 명확해졌는가 |
| 독립 lifecycle 구현 수 | 상태·timeout·종료 로직 중복이 줄었는가 |
| 중복 unsafe·FD 코드 | 동일 결함을 여러 곳에서 수정할 가능성이 줄었는가 |
| bridge·queue·task 수 | 공통화가 새로운 buffering과 복잡성을 추가하지 않는가 |
| runtime/build 의존성 변화 | 재사용 비용을 실제 graph로 확인 |
| 테스트 중복과 범위 | 테스트를 삭제한 것인지, 공통 계약으로 통합한 것인지 |
| 관련 upstream 변경의 대응 범위 | pin 변경 시 검토해야 할 파일과 계약이 줄었는가 |

정량화되지 않은 개발 시간은 추정치로 표시합니다. “A1은 낮은 비용, A4는 높은 비용” 같은 상대평가에도 어떤 항목을 포함했는지 적어야 합니다.

### 13.3 플랫폼 검증과 SLO

hosted CI에서는 해당 환경에서 수행 가능한 기능·호환성·fault test를 실행합니다. 자원 부족이나 플랫폼 제약으로 실행할 수 없는 사례는 명시적으로 `not_run`으로 남깁니다.

**qualification 호스트의 성공을 hosted CI 성공으로 대체하거나, 반대로 hosted CI의 skip을 전체 통합 실패처럼 해석하지 않습니다.**

현재 계획의 status·termination 응답 목표와 장시간 측정 절차는 별도 통합 검증으로 유지합니다. DevGuard standalone의 qualification은 CodeSpace의 승인·replay·출력·control 경로를 검증한 것이 아닙니다.

검증 결과에는 다음을 함께 기록합니다.

```text
CodeSpace source SHA
DevGuard client source SHA
daemon/helper release 및 hash
Codex SHA
backend 식별
wire/capability
운영 정책
OS/architecture/실행 호스트
실제 수행한 테스트와 not_run 사유
원시 로그·보고서 hash
```

문서만 바꾸었는데 DG-1 qualification을 무효화할 필요는 없습니다. 반면 runtime artifact를 바꾸었다면 기존 검증을 새 구현의 검증으로 재사용했다고 표현하지 않습니다.

## 14. 문서별 수정 명세

### 14.1 DevGuard PR #8

| 문서 | 개정 내용 |
|---|---|
| `docs/design.md` 및 한국어 대응 | 공통 실행 계약, D1–D3, 현재 기본 의존성과 조건부 재사용 정책 명시 |
| `docs/planning/decisions.md` | 신규 실행 계층 결정과 재사용 정책 추가. 과거 결정을 현재 지침처럼 중복 제시하지 않도록 superseded 범위 표시 |
| `docs/planning/codespace-integration.md` | LaunchPlan, 회수 소유권, FD/guard, 출력 계약, 실패 표, D2 종료 조건 반영 |
| `docs/planning/milestones/CS-RG.md` | C00·C09와 개정 PR 순서, 각 작업의 명확한 산출물 반영 |
| `docs/planning/consumer-readiness.md` | R3 진입 조건에 common lifecycle·mixed-mode 안전성·D2 결정 완료 추가 |
| `docs/planning/verification.md` | 테스트 행렬, evidence 수준, dependency 측정, backend 적합성 규칙 |
| `docs/planning/pr-delivery.md` | 이번 설계 개정의 근거, 문서 PR과 코드 PR 구분, 최종 head 검증 순서 |
| `docs/planning/README.md` | 바뀐 의사결정과 작업 구조 요약 |
| `docs/contracts.md` | 현재 구현의 사실만 기술. 미래 API를 이미 구현된 계약처럼 넣지 않음 |
| `README.md`, `docs/milestones.md` | 설계 개정 및 후속 통합 단계 요약 |
| `AGENTS.md` | 현재 설계 기준과 historical approval의 관계, adaptation 규칙 연결 |
| `docs/translations.json` | 실제 변경한 문서 쌍의 검토·hash 갱신 |

기존 `decisions.md`에는 과거 `service-exec` 설명과 이후 구현 보정이 함께 존재합니다. 이번 개정에서는 **현재 적용 규칙과 역사적 설명을 분리**하여, 처음 읽는 구현자가 오래된 문장을 현재 명령으로 오해하지 않게 해야 합니다.

#### 보존 대상

`docs/design.ko.md`와 그 역사적 승인 checksum은 변경하지 않습니다.

대신 편집 기준 설계와 결정 기록에 다음을 명시합니다.

```text
역사적 승인본: 당시 결정을 보존
현재 편집 기준: 이번 사용자 지시에 따른 개정 사항 적용
구현 계약 문서: 실제 구현된 동작만 기술
milestone 상태: 실제 구현·검증 진행에 따라 갱신
```

설계 문서를 수정했다고 CS-RG를 `implemented`나 `qualified`로 변경하지 않습니다.

### 14.2 CodeSpace의 대응 문서 PR

DevGuard PR #8만 바꾸면 CodeSpace의 기존 roadmap이 다시 오래된 설명으로 남습니다. 따라서 별도의 CodeSpace 대응 문서 변경을 같은 개정 작업에 포함합니다.

| 대상 | 변경 |
|---|---|
| `docs/devguard-integration.md` | 새 설계 revision과 작업 순서 연결 |
| `docs/architecture.md` | 공통 실행 조정과 backend 소유권 구분 |
| `docs/execution-substrate.md` | 종료 관찰·회수·출력·resource 수명 분리 |
| `docs/codex-reuse.md` | 고수준 spawn과 저수준 utility 재사용 구분 |
| `docs/upstream-update.md` | 제한적 adaptation 정책과 재검토 절차 |
| dependency 검사 규칙 | 새 resource-client 및 backend 의존성 경계 |
| CI 선택 정책 | 새 pin·실행 계약·guard·backend 파일의 영향 범위 반영 |

새 문서의 불변 링크는 **실제 커밋이 생성된 후 그 SHA로 기록**합니다. 미리 존재하지 않는 revision이나 PR 번호를 만들지 않습니다.

## 15. PR #8 병합 전 완료 기준

PR #8은 다음 조건을 충족해야 문서 개정 완료로 볼 수 있습니다.

**첫째, 검토의 권한과 목적이 바뀌어야 합니다.** 사용자의 현재 지시를 설계 재평가의 근거로 기록하고, 과거 승인 여부를 대안 기각 근거로 사용하지 않습니다.

**둘째, F1이 실행 계약 전체를 다뤄야 합니다.** reap만이 아니라 FD 전달, 동시 spawn, 출력 bridge, 유지보수 분기를 포함합니다.

**셋째, D1–D3가 서로 독립된 결정으로 남아야 합니다.** CodeSpace가 child를 직접 소유한다는 결론에서 DevGuard의 영구 Codex 금지나 모든 `off` backend 교체를 자동 도출하지 않습니다.

**넷째, D2가 필수 종료 게이트가 되어야 합니다.** 최종 qualification 전에 실제 통합 또는 제한적 유지 결정을 완료하도록 작업 계획에 반영합니다.

**다섯째, 문서가 검증되지 않은 주장을 하지 않아야 합니다.** “현재 모든 계약 충족”, “몇백 줄이면 해결”, “최신 pin이면 해결”, “ProcessDriver를 바로 사용 가능” 같은 문장을 제거하거나 증거 수준에 맞게 한정합니다.

**여섯째, 설계·구현·qualification 상태를 구분해야 합니다.** PR #8은 구현과 검증의 방향을 바꾸지만, 아직 수행하지 않은 runtime 검증을 완료로 표시하지 않습니다.

문서 검증은 기존 checker와 회귀 테스트를 실행하고, 변경된 한국어 대응과 hash를 갱신합니다. PR head에서 수행한 결과와 병합 후 main 결과는 별도로 기록합니다.

## 최종 개정 방향

이번 개정은 **“앞선 검토의 기존안을 유지하되 설명만 보강하는 작업”도 아니고, “Codex를 공통 upstream으로 만들기 위해 두 저장소를 강제로 바꾸는 작업”도 아닙니다.**

다음 네 가지를 실제 설계 결정으로 만드는 작업입니다.

> **1. CodeSpace의 실행 상태와 소유권 조정은 공통화한다.**\
> backend 차이가 승인·timeout·종료·출력·정리 로직의 중복으로 번지지 않게 한다.
>
> **2. `required`의 안전한 실행을 먼저 확보하되, `off` 분기는 무기한 방치하지 않는다.**\
> CS-RG 최종 검증 전에 통합 또는 제한적 유지의 근거를 확정한다.
>
> **3. Codex 재사용은 기능 이름이 아니라 실제 계약으로 판단한다.**\
> 맞는 구현은 재사용하고, 필요한 범위의 adaptation은 추적 가능하게 관리한다.
>
> **4. DevGuard의 현행 Codex-free 구현은 현재의 공학적 선택으로 유지한다.**\
> 그러나 이를 저장소 전체의 영구적인 설계 금지로 고정하지 않으며, 재사용 이익이 구체화되는 조건을 문서화한다.

**이렇게 개정하면 사용자가 제기한 유지보수 최적화 문제가 단순한 장기 희망사항이 아니라, 이번 CS-RG의 구조·작업 순서·완료 조건에 직접 포함됩니다.**

## 부록: 사실 주장의 근거

증거 수준: 2026-09-27에 고정 revision에서 수행한 코드 검토입니다. 줄 번호는 해당 revision 기준입니다. 이 개정을 위해 runtime 시험을 실행하지 않았습니다.

| 주장 | 근거 |
| --- | --- |
| 고정된 Codex 고수준 spawn은 자체 task에서 child를 회수한다 | Codex `6b9826e`: `codex-rs/utils/pty/src/pty.rs:244-253`·`:378-387`, `pipe.rs:280-299` |
| 고정된 `ProcessDriver` bridge는 lag로 빠진 항목을 건너뛰고, `ProcessHandle`의 Drop은 종료를 수행한다 | Codex `6b9826e`: `codex-rs/utils/pty/src/process.rs:427`·`:273-277` |
| CodeSpace의 pipe timeout·종료 경로는 waiter 외에도 `try_wait`를 호출한다 | CodeSpace `b6e7ed2`: `crates/runner/src/process.rs:366`(timeout), `:684`(`reap_child`), `:714`(`request_kill`) |
| `e94d214`에서 대응시킨 CodeSpace runtime 경로는 patch helper의 시험 재사용과 build-environment 보고 key 두 개를 제외하면 `b6e7ed2`에서 바뀌지 않았다 | CodeSpace에서 `git diff e94d214 b6e7ed2 -- crates scripts/validate-upstream.py third_party` |
| `HelperCommand`는 설정용 `Command`를 제공하고 자체 `spawn()`으로 생성하며, 이 함수가 `spawn_guard`를 획득한다. `helper_command`도 grant의 FD를 만드는 동안 같은 guard를 획득한다. guard는 표준 `Mutex`다 | DevGuard `395315d`: `crates/client/src/launch.rs:130-155`, `:176`, `:29-37` |
| `LaunchOutcome::Started`는 READY와 exec 사이에 종료된 helper도 같은 값으로 나타낸다 | DevGuard `395315d`: `crates/client/src/launch.rs:69-73` |
| CLI는 회수 전에 관찰하고, 관찰 실패를 기록한 뒤 회수하고, 회수 후 다시 관찰한다 | DevGuard `395315d`: `crates/cli/src/exec.rs:345-356`, `:470-478` |
| helper는 PID·process group·상속 FD를 유지한 채 QoS clamp 아래에서 재실행하며, 이미 group을 이끄는 process에서 scope root 전환은 아무 동작도 하지 않는다 | DevGuard `395315d`: `crates/launch/src/main.rs:172-189`, `crates/macos/src/root.rs:13-25`, `:32-36` |
| helper 자체의 종료 상태는 125·126·127이다 | DevGuard `395315d`: `crates/client/src/launch.rs:39-46` |
| `AbandonLaunch`는 claim되지 않은 grant만 `NoHelperCreated`로 반환한다 | DevGuard [계약](contracts.md)의 launch 대조 |
| 표준 `Mutex`를 보유한 스레드가 다시 잠그면 정상 반환하지 않으며, `pre_exec` closure는 fork 후의 제한된 환경에서 등록 순서대로 실행된다 | Rust 표준 라이브러리 문서의 [`Mutex::lock`](https://doc.rust-lang.org/std/sync/struct.Mutex.html#method.lock)과 [`CommandExt::pre_exec`](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec) |
