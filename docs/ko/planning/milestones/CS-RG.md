# CS-RG — CodeSpace 소비와 관제 보호

소유 저장소: CodeSpace. 상태: `not-started` / `not-run`. 진입: DG1-C12의 검증된 macOS 조합. DG1-C12는 완료되었고, macOS qualification을 마친 release `0.1.0-5daee5d-b3fa569e`가 pin 후보다. 계약 검토·adapter 초안은 앞서 준비할 수 있지만 일상 `required` 소비 자격을 앞당기지 않는다. 완료: CSRG-C09의 backend 결정 뒤 남은 head에서 고정 client·artifact·wire 조합이 기존 권한/승인/workspace/PTY 계약과 관제 SLO를 함께 통과.

CodeSpace 확인 기준은 `b6e7ed22e2c730ac987297455e250cbd6e8e8b0c`이며 초기 분석 기준 `e94d21475643608ad2a466256fb57266b86faa47`은 이력으로 보존한다. 아래 ID·commit 제목·PR은 **예정 값**이며 DevGuard 저장소의 runtime 구현으로 집계하지 않는다. `crates/resource-client`와 qualification harness는 예정 경로다. [설계 개정 1](../../design-revision-1.md)(2026-09-27)이 CSRG-C00과 CSRG-C09를 추가하고 나머지 작업의 실행 계층을 개정했다.

| 예정 PR | 작업 | 선행 PR | 활성화 경계 |
| --- | --- | --- | --- |
| CSRG-P0 | CSRG-C00 | DG1-P6 | 실행 경계 적합성만 검증; 제품 동작 변경 없음 |
| CSRG-P1 | CSRG-C01, CSRG-C02 | CSRG-P0 | pin·등록·operator 설정; 기본 off, 실행 경로 미완성 시 required startup 거절 |
| CSRG-P2 | CSRG-C03, CSRG-C04 | CSRG-P1 | 공통 supervisor·prepare·cancel·승인·불확실성을 함께 구현 |
| CSRG-P3 | CSRG-C05, CSRG-C06 | CSRG-P2 | transport 분리와 출력·총량/replay 정리 함께 활성화 |
| CSRG-P4 | CSRG-C07, CSRG-C09 | CSRG-P3 | 모드별 동등성·장애 검증과 기존 backend의 통합 또는 제한적 유지 결정 |
| CSRG-P5 | CSRG-C08 | CSRG-P4 | C09 뒤 최종 head의 upstream·제품 SLO qualification 후 도입 |

현재 명령은 CodeSpace의 `python3 scripts/validate-upstream.py all` 및 macOS의 별도 `python3 scripts/validate-upstream.py macos-core dependencies`다. 올바른 Codex submodule·기존 toolchain·Linux 환경이 필요하며 macOS의 Linux skip은 성공 증거가 아니다. 각 작업은 관련 기존 시험도 함께 실행한다. 아래 `python3 scripts/qualify-devguard.py <suite>`는 **미제공 예정 명령**이며 해당 구현 PR이 재현 fixture와 함께 제공한다.

## DG-1 소비 인터페이스

2026-09-26 검토에서 이 계획을 DG-1이 구현한 소비 인터페이스([계약](../../contracts.md))와 대조했다. 아래 모든 작업은 이 인터페이스를 따른다.

- **세션.** 모든 frame에는 idle 대기를 포함해 250ms 절대 기한이 있다. 실행 소유자는 단계마다 새 세션을 연다: 연결, `Hello`, `Authenticate`, 자신의 instance ID 하나로 `Register`, 그리고 요청 하나. 별도의 `service-exec` 경로는 없으며 Gateway는 소비자 자격을 `CredentialHandoff`로 UDS worker에 넘긴다.
- **실행.** `Admit`은 5초 기한의 Prepared attempt를 기록하고 `BeginLaunch`가 이를 commit한다. 일회용 permit은 첫 응답에만 들어 있다. 소유자는 `HelperCommand`로 `devguard-launch`를 직접 child로 시작한다. `HelperCommand`는 permit과 transcript를 private descriptor로 전달하고 `spawn_guard` 아래에서 spawn한다. `helper_command`와 `HelperCommand::spawn`이 이 guard를 직접 잡으므로 호출자는 그 주위에서 guard를 잡지 않는다. transcript는 `failed`, `refused`, `ready`, `exec_failed`를 보고한다.
- **회수 전 관측.** 소유자는 root 종료를 회수하지 않고 감지한 뒤(`WNOWAIT`를 준 `waitid`) `Observe`를 호출하고 그다음 회수한다. 먼저 회수된 root의 잔존 process는 영구 추적 상실이 되며, attempt는 재부팅 전까지 Suspect로 남아 예산을 계속 점유한다.
- **helper 미생성.** permit 응답 유실, helper spawn 실패, READY 전 helper 종료는 `AbandonLaunch`로 보고한다. 그러면 authority가 claim되지 않은 grant를 `NoHelperCreated`로 해제하며, 이 해제가 executable이 시작되지 않았다는 증거다. 보고 자체는 증거가 아니며 claim된 grant는 보고로 해제할 수 없다.
- **provisioning.** 소비자는 `host.toml`의 운영자 설정이다: role, generation, 자격, instance 상한, 그리고 `control_service` 소비자에 한해 정적 제어 예약. 서비스는 시작할 때만 이 설정을 읽으며, 재시작은 commit된 attempt를 Suspect로 남긴다.

## 실행 소유권

고정된 Codex 고수준 spawn은 변경 없이 DG-1 관리 실행을 담을 수 없다. pipe·PTY spawn 함수가 child를 내부에서 회수하고 이미 상속 가능한 descriptor만 유지하기 때문이다. 이는 회수 소유권과 FD 전달 계약의 불일치이며 Codex 코드를 쓸 수 없다는 증거가 아니다. 아래 모든 작업은 설계 개정 1의 [실행 소유권 규칙](../codespace-integration.md#실행-소유권)을 따른다.

- **단일 회수자.** 각 child는 supervisor 객체 하나가 소유한다. `required` 경로에서는 그 밖의 어떤 코드도 `wait`, `try_wait`, `waitpid`를 호출하지 않으며 종료·timeout·shutdown은 의도를 전달한다. 스스로 회수하는 legacy `off` backend(`BackendReaped`)는 회수되지 않은 종료 상태를 제공한다고 표시하지 않으며, `required` 경로는 `OwnerControlledReap`이다.
- **일회성 launch plan.** `PreparedExecution`은 실행 identity·의미·슬롯·workspace·자원 상태·최초 기한을 소유하고 launch plan은 한 번만 소비한다. 복제하거나 argv로 다시 만들지 않으며, 취소된 task의 Drop이 lease를 반환했다고 가정하지 않는다.
- **spawn 보호.** 각 spawn 경로의 보호 책임자는 정확히 하나다: `HelperCommand` 자신, 공통 guard 또는 검증된 동등 보호. guard는 descriptor 생성·상속 설정·spawn만 보호한다.
- **출력.** CodeSpace 출력 수집기 하나와 상한 있는 보존을 쓴다. 손실량을 모르면 `output_lost=false`로 보고하지 않는다.
- **backend 결정.** legacy `off` backend는 초기 호환성을 위해 남을 수 있다. CSRG-C08 전에 CSRG-C09가 통합과 제한적 compatibility backend 유지 중 하나를 결정한다.

이 작업을 작은 PTY opener 추가로 설명하지 않는다. 코드 양이 아니라 검증해야 할 경쟁·실패 경로로 평가한다.

## 작업 단위

### CSRG-C00 — 관리 실행 경계 적합성 검증

- 소유/예정 PR: CodeSpace / CSRG-P0. 예정 제목: `test(runner): verify the managed execution boundary`.
- 문제 → 동작: 공통 실행 계약을 쌓기 전에, 현재 pin에서 최소 구현으로 관리 helper를 PTY에서 실행할 수 있고 descriptor가 안전하며 root 종료 뒤 관찰·회수 순서를 소유자가 제어함을 입증한다. 모든 spawn·회수 경로를 목록화한다.
- 선행: DG1-C12(완료). CodeSpace 확인 기준 `b6e7ed2`, Codex pin `6b9826e`, pin 후보 source의 DG-1 `HelperCommand`.
- 대상/산출물: Gateway와 UDS Runner의 모든 child 생성·회수 경로마다 소유자와 보내는 메시지를 적은 소유권 표(pipe·PTY spawn, 종료 감시, timeout, `request_kill`, workspace 단위 종료, shutdown, backend Drop, task 취소, 오류 정리, patch·sandbox helper, 보조 명령, worker 생성, 시험 helper); `HelperCommand`를 쓰는 최소 관리 PTY; legacy Codex PTY와 중복 client 버전을 포함한 spawn guard·descriptor 적합성 보고서; 연결·`Authenticate`·`Register`·`Admit`까지의 end-to-end 기한 전파; 제안 예산 1초에 대한 회수 전 `Observe` 실측.
- 불변 조건: child당 회수 책임자 하나이며 소유자 밖의 회수 호출 없음; `helper_command`나 `HelperCommand::spawn` 주위에서 `spawn_guard`를 잡지 않음; private descriptor가 관계없는 child나 payload에 닿지 않음; timeout으로 감싼 blocking 호출은 실행 중인 동안 계속 추적; 검증 코드는 후속 공통 시험으로 흡수하거나 삭제하며 제품의 네 번째 backend로 남기지 않음; 제품 동작 변경 없음.
- 시험: 초기 크기·resize·controlling terminal·session과 group·EOF를 포함한 PTY 위의 관리 helper; 즉시 종료하는 root와 자손이 남는 root; `Observe` 실패·timeout 뒤의 회수; helper 생성 중 관계없는 동시 spawn; setup 실패 시 child·master·slave·private descriptor 정리; helper의 QoS 재실행 뒤 descriptor와 PID 의미 유지; jobserver descriptor 유지.
- 검증 명령: 현재 upstream 관련 시험 + 예정 `python3 scripts/qualify-devguard.py boundary`.
- 완료 증거: 소유권 표, payload descriptor 목록, 경로별 판정(안전·변경 필요·미지원), `Observe`와 기한 trace 실측, CSRG-C03과 CSRG-C09에 넘길 발견 사항.
- rollback: 시험 전용 코드를 되돌린다. 활성화되는 기능은 없다.
- 인계: 보고서 뒤 CSRG-C01을 시작한다. CSRG-C03은 소유권 표와 입증된 PTY·descriptor 구성을, CSRG-C09는 legacy 경로 발견 사항을 받는다.

### CSRG-C01 — 검증된 pin과 작은 client adapter

- 소유/예정 PR: CodeSpace / CSRG-P1. 예정 제목: `feat(resources): consume a qualified DevGuard client revision`.
- 문제 → 동작: 계획 revision 인용을 실제 runtime dependency로 오해하지 않도록 DG1 qualification에 연결된 전체 source SHA와 작은 client 경계를 도입한다.
- 선행: CSRG-C00과 DG1-C12(완료). 선택 artifact·license·wire/capability 조합, 플랫폼별 지원표와 CI executor 확보. 후보는 source `5daee5d`로 빌드한 release `0.1.0-5daee5d-b3fa569e`이며 소비 crate는 `395315d`와 같다.
- 대상/산출물: 예정 `crates/resource-client`, dependency lock/provenance(client와 helper 출처를 분리 기록), 기존 `scripts/upstream_dependencies.py`와 의존 경계 검사 확장; 설치된 current release에서 찾은 `devguard-launch` 경로; 격리 시험 authority를 위한 dev 전용 `devguard-daemon` `test-fixtures` 기능.
- 불변 조건: DevGuard core에 CodeSpace/Codex 의존 없음; client가 Codex 전이 의존을 들여오지 않음; CodeSpace 권한 결정을 외부 authority에 이전하지 않음; 설계 개정 1은 Codex pin을 유지.
- 시험: 지원 client/daemon handshake 정상; 변경/미지원 pin·누락 capability·실행 중 authority와 다른 release의 helper 거절; 구·신 client 동시 소비와 reconnect 중 동일 attempt 재전송; runtime·build 의존 graph의 Codex crate 검사.
- 검증 명령: 현재 upstream `policy dependencies` stage + 예정 `python3 scripts/qualify-devguard.py client`.
- 완료 증거: 전체 source SHA, artifact hashes, runtime·build·dev 의존을 구분한 license/의존 graph diff, 지원 조합 결과. pin 값은 qualification 후 실제 값으로 채운다.
- rollback: 신규 소비를 끄기 전 live lease 대조; 호환 client로만 되돌리고 unsupported artifact를 자동 설치하지 않는다.
- 인계: CSRG-C02에 client API와 버전별 error/capability 표. pin과 설정을 같은 PR에서 검토한다.

### CSRG-C02 — 운영자 설정과 Runner 단일 등록

- 소유/예정 PR: CodeSpace / CSRG-P1. 예정 제목: `feat(resources): register the execution owner and expose required policy`.
- 문제 → 동작: 개발 설정 존재와 실제 runtime 소비를 구분하고 운영자가 기본 `off` 또는 명시적 `required`를 선택한다.
- 선행: CSRG-C01. 실제 호스트 authority. 운영자가 provisioning한 `codespace` 소비자(role `control_service`, generation, private 자격 파일, instance 상한, 정적 제어 예약)를 점유 중인 예산이 없을 때 적용. InProcess/UDS 선택 완료.
- 대상/산출물: server config/start_runner/runtime, Runner init와 capability, domain resource 오류, Gateway에서 UDS worker로의 `CredentialHandoff`, 단계별 등록 세션, workspace `resources` 설정(profile, 요청 예산, 최소 수준), 한 프로세스 안의 legacy·관리 spawn 규칙.
- 불변 조건: 실행 소유자당 instance 하나(InProcess는 Gateway PID, UDS는 worker PID)를 한정된 세션마다 다시 등록하며 launcher는 등록하지 않음(instance와 세션은 다름); Gateway+Runner 예약 하나; caller identity 조작 불가; 자격은 payload에 닿지 않음; DG-1이 `ResourcePolicyUnsupported`를 보고하는 곳(Linux 포함)에서 required는 닫힌 채 실패; 한 프로세스의 legacy·관리 spawn은 같은 보호 객체를 공유하며 그렇지 않은 혼합 조합은 미지원; 공개 MCP 도구명 유지.
- 시험: 두 모드 정상 등록/off 동작; required인데 daemon/권한/capability 부족 시 거절; 작업 용량을 넘는 resource 설정은 load 시 거절; worker startup 경쟁·슬롯 중복·pipe/PTY 자격 누출·한 프로세스 안의 legacy/관리 혼합 spawn.
- 검증 명령: 현재 upstream 관련 설정/PTY 시험 + 예정 `python3 scripts/qualify-devguard.py registration`.
- 완료 증거: mode별 PID/Principal/정적 예약 대응, requested/supported/applied 분리, 부족/서비스 장애/미지원/불확실 오류 표.
- rollback: 신규 등록 중지 후 lease/instance retire, 호환 설정으로 복귀. required에서 조용한 off fallback 금지.
- 인계: CSRG-C03에 등록 완료 Runner와 private 전달 API; P1만으로 runtime qualification을 선언하지 않는다.

### CSRG-C03 — spawn 전 슬롯과 단일 회수자의 실행 감독

- 소유/예정 PR: CodeSpace / CSRG-P2. 예정 제목: `feat(runner): supervise executions with pre-spawn slots and one reaper`.
- 문제 → 동작: spawn 후 max process 검사를 spawn 전 permit으로 옮기고, 모든 회수 경로를 실행마다 하나인 supervisor로 통합하며, 준비 guard가 슬롯·자원·workspace 정리를 소유하게 한다.
- 선행: CSRG-C02. 인가와 기존 workspace FIFO 획득 후 실행 슬롯과 DevGuard 준비를 수행할 수 있는 Runner API, CSRG-C00 소유권 표.
- 대상/산출물: `crates/runner/src/process.rs`의 실행 조정자와 supervisor, backend capability(`BackendReaped` 또는 `OwnerControlledReap`), runner trait/wire, `PrepareExec`(`Admit`)/cancel DTO, 준비 만료 guard; 관리 실행은 Codex spawn adapter 밖에서 Runner가 소유한 Unix transport(pipe·PTY)와 `HelperCommand`로 시작.
- 불변 조건: 무슬롯 spawn 0; process_id/attempt 의미 고정(argv·cwd·환경·tty·workspace·정책의 execution digest); 슬롯·resource lease·완료 출력 보존은 독립 수명; 승인 소비 전 준비; child당 회수 책임자 하나이며 timeout·`request_kill`·workspace 단위 종료·shutdown은 의도를 전달; 회수 전 관측(`WNOWAIT`를 준 `waitid`, 예산 안의 `Observe`, 회수); backend는 보장하지 못하는 capability를 표시하지 않음; 다른 모든 spawn은 spawn 보호 규칙을 따름.
- 시험: 정상 prepare→실행; 슬롯 포화·예산 부족·prepare timeout; root 종료 뒤 잔존 process 추적 유지; 동시 9번째 요청·취소와 만료·응답 유실 시 permit 회수 경쟁; waiter 취소·backend Drop·`ECHILD`·이중 회수 방지; 종료와 경쟁하는 timeout·terminate·shutdown.
- 검증 명령: 현재 upstream process/workspace 시험 + 예정 `python3 scripts/qualify-devguard.py prepare`.
- 완료 증거: 실제 spawn 수와 슬롯 상한, 경로마다 회수 책임자가 하나인 최종 소유권 표, 미시작 경로의 정확한 guard 정리, committed/unknown 회계가 유지되는 trace.
- rollback: prepare 신규 진입 차단·미실행 예약 취소; committed scope는 관측 후 회수. 예전 spawn 후 검사로 live 실행을 이관하지 않는다.
- 인계: CSRG-C04와 같은 PR에서 승인 전이와 실행 commit을 완성하며 준비 전용 반쪽 기능을 활성화하지 않는다.

### CSRG-C04 — ExecPrepared와 승인·미시작 증거

- 소유/예정 PR: CodeSpace / CSRG-P2. 예정 제목: `feat(approvals): dispatch prepared attempts without replaying uncertain work`.
- 문제 → 동작: prepare 거절이 승인 hold를 소비하거나 응답 유실이 재실행으로 이어지지 않도록 durable attempt와 approval resume를 연결하고, launch plan을 한 번만 소비한다.
- 선행: CSRG-C03. 준비된 슬롯/lease, client 단계별 결과, 승인 row와 workspace lease identity.
- 대상/산출물: `server/src/mcp.rs` exec/resume, `store/src/approvals.rs`, `ExecPrepared` wire(`BeginLaunch`와 helper transcript), transcript 단계별 승인 처리, 미시작 오류 매핑·migration fixture.
- 불변 조건: prepare 성공 후 mark_resuming; launch plan은 한 번만 소비하고 argv로 다시 만들지 않음; 동일 attempt의 확인된 거절/NoHelperCreated만 승인 재사용; READY/confirmed와 executable 성공 분리; `Admit` 응답 유실은 같은 attempt를 조회·취소하고, `BeginLaunch` 응답 유실은 그 attempt를 유지하며 helper가 존재할 수 없을 때만 `AbandonLaunch`로 보고; `AbandonLaunch` 호출 자체가 아니라 authority의 `NoHelperCreated` 해제가 미시작 증거; claim된 grant는 scope로만 정리; READY 뒤 `exec_failed`는 승인을 자동 재사용하지 않음; 종료 코드 125~127로 helper 단계를 추론하지 않음.
- 시험: 정상 confirmed·승인 소비; prepare 거절 후 queued, READY 후 executable 실패는 구분; `Admit`·`BeginLaunch` 응답 유실과 transcript 단계별 처리; 이중 resume·commit 응답 유실·취소 경쟁에서 중복 spawn 0.
- 검증 명령: 현재 upstream approvals/integration 시험 + 예정 `python3 scripts/qualify-devguard.py approval`.
- 완료 증거: approval→process→attempt 대응, unknown 유지, 재사용 허용/금지 matrix와 DB version fixture, 압력 거절을 구분한 DG-1 오류 코드 전체 대응표.
- rollback: 신규 resume 중지, dispatching/unknown 대조 후 호환 버전으로 복귀; 승인 row를 일괄 queued로 재설정하지 않는다.
- 인계: CSRG-C05에 관제 op와 실행 op의 확정·불확실 응답 계약을 전달한다.

### CSRG-C05 — control/data 전송과 처리 여유

- 소유/예정 PR: CodeSpace / CSRG-P3. 예정 제목: `feat(runner): reserve transport and dispatch capacity for control`.
- 문제 → 동작: 현재 직렬 dispatch와 공유 writer 때문에 느린 stdin/큰 응답이 상태·종료를 막는 경로를 분리한다.
- 선행: CSRG-C04. 인증된 하나의 Runner session, bounded execution executor와 정적 제어 예약.
- 대상/산출물: `runner/src/wire.rs`와 UDS client, control/data 채널 pairing·별도 dispatch budget·writer·callback 감사.
- 불변 조건: public `process_status`/`terminate_process` 유지; shared mutex/callback으로 data 대기를 control에 전파 금지; spawn과 회수 전 `Observe`의 지연을 control 경로에 전파 금지; 기존 모드 disconnect 계약 보존.
- 시험: 정상 두 lane 인증; 잘못된 pairing/half-open/reader 정지; 대량 출력·느린 stdin·작업 포화·느린 spawn·느린 `Observe` 중 상태/종료, event callback 지연 경쟁.
- 검증 명령: 현재 upstream wire/disconnect 시험 + 예정 `python3 scripts/qualify-devguard.py control-lanes`.
- 완료 증거: 각 큐·동시 처리·byte 상한과 ownership 표, control 지연 trace와 공유 lock 보유 시간.
- rollback: 신규 session을 닫고 기존 모드 정책대로 정리; P1 독립 복구 모드의 생존 정책을 여기서 암묵 도입하지 않는다.
- 인계: CSRG-C06과 함께 전체 buffer/replay/수명 상한을 만족할 때만 분리 경로를 활성화한다.

### CSRG-C06 — 진행 중 replay·상한·수명 이벤트

- 소유/예정 PR: CodeSpace / CSRG-P3. 예정 제목: `feat(runner): bound inflight replay buffers and lifecycle delivery`.
- 문제 → 동작: 완료 응답만 캐시하는 replay를 진행 중 attempt까지 확장하고 대기/응답 누적 byte·보존 시간을 제한하며, bridge 내부 손실을 포함해 출력 손실을 회계한다.
- 선행: CSRG-C05. request/attempt별 single-flight ownership, control lane 여유, 완료 출력 보존 정책.
- 대상/산출물: wire replay registry, 전체 byte 상한이 있는 출력 수집기 하나, bounded stdin/output writer, lifecycle 이벤트 및 workspace release callback, 알 수 없는 출력 손실을 보고하는 데 필요한 protocol 변경.
- 불변 조건: 진행 중 중복 dispatch 0; 256KiB 출력 ring 등 기존 사용자 계약 보존; cache evict가 새 실행 허용 아님; 자원 회수와 출력 TTL 분리; 의도적 보존 손실과 알 수 없는 transport 손실을 구분하고 손실량을 모르면 `output_lost=false`로 보고하지 않음; Suspect attempt(탈출·추적 상실)는 재부팅 전까지 예산 점유로 표시.
- 시험: 정상 replay·종료 이벤트; 느린 reader·큰 응답·bridge lag·종료 뒤 tail 보존·버퍼 초과 거절; in-flight retry/완료/evict/disconnect 경쟁과 lease/slot 누수.
- 검증 명령: 현재 upstream replay/process 시험 + 예정 `python3 scripts/qualify-devguard.py replay-bounds`.
- 완료 증거: 모든 큐의 수·byte·TTL 및 memory peak, 동일 결과 재관측, 이벤트 누락 시 대조 경로, 작업 포화 중 control 성공.
- rollback: 신규 실행 차단 후 in-flight 표를 drain, terminal identity 보존. cache 삭제로 attempt tombstone을 대신하지 않는다.
- 인계: CSRG-C07이 검사할 transport·수명 불변 조건과 최악 부하 fixture를 전달한다.

### CSRG-C07 — 실행 모드 동등성과 장애

- 소유/예정 PR: CodeSpace / CSRG-P4. 예정 제목: `test(resources): exercise pipe PTY and Runner failure parity`.
- 문제 → 동작: 한 경로 성공을 전체 지원으로 확대하지 않고 `off`/`required` × pipe/PTY × InProcess/UDS와 한 프로세스 안의 legacy·관리 혼합 실행의 동일 의미를 입증한다.
- 선행: CSRG-C06. qualification 호스트에 설치된 검증 authority/helper, DG-1 `test-fixtures`로 만든 mode별 격리 fixture(합성 probe이며 OS 증거 아님), 기존 disconnect 동작 expected matrix.
- 대상/산출물: runner/server/PTY integration tests, fault injector, 자격 FD payload 검사와 mode별 결과.
- 불변 조건: 권한·workspace FIFO·approval·resize·exit 계약 유지; DevGuard 장애 중 기존 조회/종료는 handle로 처리; unknown 자동 재실행 0; 경로별 단독 성공만으로 spawn·descriptor 보호를 검증했다고 보지 않음.
- 시험: 자원 모드·transport·Runner 모드의 모든 조합과 한 프로세스 안의 혼합 실행; helper/daemon 실패·unsupported requirement·자손 잔류·group을 벗어나는 workload(`setsid`나 background group); 승인 resume/취소/replay/종료 경쟁, 기존 모드 연결 소실 회귀.
- 검증 명령: 현재 upstream `integration macos-core`는 해당 platform에서 구분 실행 + 예정 `python3 scripts/qualify-devguard.py parity`.
- 완료 증거: mode별 실행 사례 수, failure 결과·FD 누출 0·중복 0, 미지원/미실행 조합을 skip 이유와 함께 기록. 각 사례의 실행 위치도 기록한다. 기본 1 CPU 제어 예약은 3-CPU hosted runner의 작업 용량을 0으로 만들므로 hosted macOS는 fixture만 실행하거나 `not_run`으로 기록한다.
- rollback: 실패한 mode는 지원표에서 제외하고 required 활성화를 막는다. 전체 제품 합격으로 축약하지 않는다.
- 인계: 검증된 runtime mode/fixture/pin 조합만 CSRG-C09를 거쳐 CSRG-C08로 전달한다.

### CSRG-C08 — upstream 회귀와 관제 SLO

- 소유/예정 PR: CodeSpace / CSRG-P5. 예정 제목: `test(qualification): qualify the pinned DevGuard consumer combination`.
- 문제 → 동작: 독립 DG-1 합격과 실제 CodeSpace 승인·replay·관제 경로 합격을 별도의 제품 증거로 남긴다.
- 선행: CSRG-C09와 DG1-C12(완료). qualification 호스트의 정확한 source/client/artifact/wire/정책·host, foreground 및 local MCP 측정 fixture.
- 대상/산출물: 제품 qualification report, existing upstream 전체 regression evidence, operator 도입/복귀 문서.
- 불변 조건: CSRG-C09 뒤 남은 최종 구현·pin·artifact 조합만 qualification; 그 조합의 Codex pin/검사 유지; idle10분/부하30분 이상/3회; 원격 RTT 분리; 임의 크기 파일 보호는 claim 범위에서 제외; hosted runner가 아니라 설치된 검증 release를 측정.
- 시험: 정상 개발 workload·MCP 관측; daemon 장애/압력/큰 출력/느린 stdin; 포화 중 replay·승인·종료 경쟁과 foreground SLO.
- 검증 명령: 현재 `validate-upstream.py all`, 별도 `macos-core dependencies` 및 예정 `python3 scripts/qualify-devguard.py codespace-macos`; Linux stage는 실제 Linux에서 별도 실행.
- 완료 증거: 모든 요구 SLO와 raw samples, exact head별 upstream 결과, 지원 조합 manifest. 각 결과에 CodeSpace·client source SHA, daemon/helper hash, Codex SHA, backend, wire/capability, 정책, host, `not_run` 사유를 함께 기록한다. macOS 통과를 Linux 강제 보호로 표시하지 않는다.
- rollback: 검증 조합의 신규 required 소비를 중지·drain 후 이전 조합으로 복귀; 실패 보고서와 pin 후보 보존.
- 인계: P1R-C01과 DGL-C01. 이 단계 완료만으로 Gateway 복구 capability를 선언하지 않는다.

### CSRG-C09 — 기존 실행 backend의 수렴 결정

- 소유/예정 PR: CodeSpace / CSRG-P4. 예정 제목: `refactor(runner): decide and converge the legacy execution backends`.
- 문제 → 동작: 공통 인터페이스 아래 중복 backend가 계속 남으면 수명주기와 spawn 보호를 두 벌 유지하게 된다. 최종 qualification 전에 legacy `off` backend를 통합하거나, 기록된 근거로 제한적 compatibility backend를 유지한다.
- 선행: CSRG-C07. C07 동등성·장애 결과, CSRG-C00의 legacy 경로 발견 사항, 유지보수 측정값.
- 대상/산출물: (A) 해당 플랫폼·transport에서 대체된 backend의 코드·분기·시험 fixture·의존성 제거, 또는 (B) 남기는 이유, 남는 플랫폼·transport·mode, 공통화된 부분과 중복된 부분, 제공하지 않는 capability, 재검토 시점, 제거 기준을 담은 결정 기록. 어느 쪽이든 유지보수 측정값(spawn 진입점, 회수 지점, 수명주기 구현, 중복 unsafe·descriptor 코드, bridge·queue·task, runtime·build 의존 변화, 시험 중복과 범위, upstream 변경 대응 범위)을 포함한다.
- 불변 조건: “기존 코드”나 “parity 통과”만으로 결정하지 않음; 분기 유지도 교체와 같은 입증 부담을 짐; 문서에 “검토했다”고 쓰는 것으로 완료하지 않음; 측정하지 않은 개발 시간은 추정치로 표시; spawn 보호 증거 없이는 legacy·관리 혼합 사용을 미지원으로 유지.
- 시험: 코드 변경 뒤 영향받는 C07 동등성 재실행; 제거한 경로의 호출·fixture·의존성이 남지 않았는지 검사; 유지하는 backend는 제공하지 않는 capability를 거절.
- 검증 명령: 현재 upstream 관련 시험 + 예정 `python3 scripts/qualify-devguard.py backends`, 영향받는 `python3 scripts/qualify-devguard.py parity` 사례 재실행.
- 완료 증거: 측정값을 포함한 결정, CSRG-C08이 측정할 최종 head, C07 재실행 결과.
- rollback: 수렴 변경을 되돌리고 CSRG-C08 전에 영향받는 C07 동등성을 다시 실행한다. 유지 결정의 번복은 새 결정 기록으로만 한다.
- 인계: CSRG-C08은 이 작업이 남긴 head만 qualification한다.
