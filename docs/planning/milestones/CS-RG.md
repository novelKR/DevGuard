# CS-RG — CodeSpace 소비와 관제 보호

소유 저장소: CodeSpace. 상태: `not-started` / `not-run`. 진입: DG1-C12의 검증된 macOS 조합. 계약 검토·adapter 초안은 앞서 준비할 수 있지만 일상 `required` 소비 자격을 앞당기지 않는다. 완료: 고정 client·artifact·wire 조합에서 기존 권한/승인/workspace/PTY 계약과 관제 SLO를 함께 통과.

CodeSpace 분석 기준은 `e94d21475643608ad2a466256fb57266b86faa47`이다. 아래 ID·commit 제목·PR은 **예정 값**이며 DevGuard 저장소의 runtime 구현으로 집계하지 않는다. `crates/resource-client`와 qualification harness는 예정 경로다.

| 예정 PR | 작업 | 선행 PR | 활성화 경계 |
| --- | --- | --- | --- |
| CSRG-P1 | CSRG-C01, CSRG-C02 | DG1-P6 | pin·등록·operator 설정; 기본 off, 실행 경로 미완성 시 required startup 거절 |
| CSRG-P2 | CSRG-C03, CSRG-C04 | CSRG-P1 | prepare·cancel·승인·불확실성을 함께 구현 |
| CSRG-P3 | CSRG-C05, CSRG-C06 | CSRG-P2 | transport 분리와 총량/replay 정리 함께 활성화 |
| CSRG-P4 | CSRG-C07, CSRG-C08 | CSRG-P3 | 모드별 동등성·upstream·제품 SLO qualification 후 도입 |

현재 명령은 CodeSpace의 `python3 scripts/validate-upstream.py all` 및 macOS의 별도 `python3 scripts/validate-upstream.py macos-core dependencies`다. 올바른 Codex submodule·기존 toolchain·Linux 환경이 필요하며 macOS의 Linux skip은 성공 증거가 아니다. 각 작업은 관련 기존 시험도 함께 실행한다. 아래 `python3 scripts/qualify-devguard.py <suite>`는 **미제공 예정 명령**이며 해당 구현 PR이 재현 fixture와 함께 제공한다.

### CSRG-C01 — 검증된 pin과 작은 client adapter

- 소유/예정 PR: CodeSpace / CSRG-P1. 예정 제목: `feat(resources): consume a qualified DevGuard client revision`.
- 문제 → 동작: 계획 revision 인용을 실제 runtime dependency로 오해하지 않도록 DG1 qualification에 연결된 전체 source SHA와 작은 client 경계를 도입한다.
- 선행: DG1-C12. 선택 artifact·license·wire/capability 조합, 플랫폼별 지원표와 CI executor 확보.
- 대상/산출물: 예정 `crates/resource-client`, dependency lock/provenance, 기존 `scripts/upstream_dependencies.py`와 의존 경계 검사 확장.
- 불변 조건: DevGuard core에 CodeSpace/Codex 의존 없음; CodeSpace 권한 결정을 외부 authority에 이전하지 않음; 현재 Codex pin 유지.
- 시험: 지원 client/daemon handshake 정상; 변경/미지원 pin·누락 capability 거절; 구·신 client 동시 소비와 reconnect 중 동일 attempt 재전송.
- 검증 명령: 현재 upstream `policy dependencies` stage + 예정 `python3 scripts/qualify-devguard.py client`.
- 완료 증거: 전체 source SHA, artifact hashes, license/의존 graph diff, 지원 조합 결과. pin 값은 qualification 후 실제 값으로 채운다.
- rollback: 신규 소비를 끄기 전 live lease 대조; 호환 client로만 되돌리고 unsupported artifact를 자동 설치하지 않는다.
- 인계: CSRG-C02에 client API와 버전별 error/capability 표. pin과 설정을 같은 PR에서 검토한다.

### CSRG-C02 — 운영자 설정과 Runner 단일 등록

- 소유/예정 PR: CodeSpace / CSRG-P1. 예정 제목: `feat(resources): register the execution owner and expose required policy`.
- 문제 → 동작: 개발 설정 존재와 실제 runtime 소비를 구분하고 운영자가 기본 `off` 또는 명시적 `required`를 선택한다.
- 선행: CSRG-C01. 실제 호스트 authority와 충분한 정적 제어 슬롯, 전달할 자격 FD. InProcess/UDS 선택 완료.
- 대상/산출물: server config/start_runner/runtime, Runner init와 capability, domain resource 오류, private PTY FD adapter. CodeSpace용 service-exec 연계 명세.
- 불변 조건: InProcess는 Gateway PID, UDS는 worker PID 단일 등록; Gateway+Runner 예약 하나; caller identity 조작 불가; payload 전 FD 닫기; 공개 MCP 도구명 유지.
- 시험: 두 모드 정상 등록/off 동작; required인데 daemon/권한/capability 부족 시 거절; worker startup 경쟁·슬롯 중복·pipe/PTY 자격 누출.
- 검증 명령: 현재 upstream 관련 설정/PTY 시험 + 예정 `python3 scripts/qualify-devguard.py registration`.
- 완료 증거: mode별 PID/Principal/정적 예약 대응, requested/supported/applied 분리, 부족/서비스 장애/미지원/불확실 오류 표.
- rollback: 신규 등록 중지 후 lease/instance retire, 호환 설정으로 복귀. required에서 조용한 off fallback 금지.
- 인계: CSRG-C03에 등록 완료 Runner와 private 전달 API; P1만으로 runtime qualification을 선언하지 않는다.

### CSRG-C03 — spawn 전 슬롯·PrepareExec·취소

- 소유/예정 PR: CodeSpace / CSRG-P2. 예정 제목: `feat(runner): reserve process slots before resource preparation`.
- 문제 → 동작: 현재 spawn 후 max process 검사를 spawn 전 permit으로 옮기고 prepare 단계에서 슬롯·자원·workspace 정리를 명확히 한다.
- 선행: CSRG-C02. 인가와 기존 workspace FIFO 획득 후 실행 슬롯과 DevGuard 준비를 수행할 수 있는 Runner API.
- 대상/산출물: `crates/runner/src/process.rs`, runner trait/wire, `PrepareExec`/cancel DTO, 준비 만료 guard.
- 불변 조건: 무슬롯 spawn 0; process_id/attempt 의미 고정; 슬롯·resource lease·완료 출력 보존은 독립 수명; 승인 소비 전 준비.
- 시험: 정상 prepare→실행; 슬롯 포화·예산 부족·prepare timeout; 동시 9번째 요청·취소와 만료·응답 유실 시 permit 회수 경쟁.
- 검증 명령: 현재 upstream process/workspace 시험 + 예정 `python3 scripts/qualify-devguard.py prepare`.
- 완료 증거: 실제 spawn 수와 슬롯 상한, 미시작 경로의 정확한 guard 정리, committed/unknown 회계가 유지되는 trace.
- rollback: prepare 신규 진입 차단·미실행 예약 취소; committed scope는 관측 후 회수. 예전 spawn 후 검사로 live 실행을 이관하지 않는다.
- 인계: CSRG-C04와 같은 PR에서 승인 전이와 실행 commit을 완성하며 준비 전용 반쪽 기능을 활성화하지 않는다.

### CSRG-C04 — ExecPrepared와 승인·미시작 증거

- 소유/예정 PR: CodeSpace / CSRG-P2. 예정 제목: `feat(approvals): dispatch prepared attempts without replaying uncertain work`.
- 문제 → 동작: prepare 거절이 승인 hold를 소비하거나 응답 유실이 재실행으로 이어지지 않도록 durable attempt와 approval resume를 연결한다.
- 선행: CSRG-C03. 준비된 슬롯/lease, client 단계별 결과, 승인 row와 workspace lease identity.
- 대상/산출물: `server/src/mcp.rs` exec/resume, `store/src/approvals.rs`, `ExecPrepared` wire, 미시작 오류 매핑·migration fixture.
- 불변 조건: prepare 성공 후 mark_resuming; 동일 attempt의 확인된 거절/NoHelperCreated만 승인 재사용; READY/confirmed와 executable 성공 분리.
- 시험: 정상 confirmed·승인 소비; prepare 거절 후 queued, READY 후 executable 실패는 구분; 이중 resume·commit 응답 유실·취소 경쟁에서 중복 spawn 0.
- 검증 명령: 현재 upstream approvals/integration 시험 + 예정 `python3 scripts/qualify-devguard.py approval`.
- 완료 증거: approval→process→attempt 대응, unknown 유지, 재사용 허용/금지 matrix와 DB version fixture.
- rollback: 신규 resume 중지, dispatching/unknown 대조 후 호환 버전으로 복귀; 승인 row를 일괄 queued로 재설정하지 않는다.
- 인계: CSRG-C05에 관제 op와 실행 op의 확정·불확실 응답 계약을 전달한다.

### CSRG-C05 — control/data 전송과 처리 여유

- 소유/예정 PR: CodeSpace / CSRG-P3. 예정 제목: `feat(runner): reserve transport and dispatch capacity for control`.
- 문제 → 동작: 현재 직렬 dispatch와 공유 writer 때문에 느린 stdin/큰 응답이 상태·종료를 막는 경로를 분리한다.
- 선행: CSRG-C04. 인증된 하나의 Runner session, bounded execution executor와 정적 제어 예약.
- 대상/산출물: `runner/src/wire.rs`와 UDS client, control/data 채널 pairing·별도 dispatch budget·writer·callback 감사.
- 불변 조건: public `process_status`/`terminate_process` 유지; shared mutex/callback으로 data 대기를 control에 전파 금지; 기존 모드 disconnect 계약 보존.
- 시험: 정상 두 lane 인증; 잘못된 pairing/half-open/reader 정지; 대량 출력·느린 stdin·작업 포화 중 상태/종료, event callback 지연 경쟁.
- 검증 명령: 현재 upstream wire/disconnect 시험 + 예정 `python3 scripts/qualify-devguard.py control-lanes`.
- 완료 증거: 각 큐·동시 처리·byte 상한과 ownership 표, control 지연 trace와 공유 lock 보유 시간.
- rollback: 신규 session을 닫고 기존 모드 정책대로 정리; P1 독립 복구 모드의 생존 정책을 여기서 암묵 도입하지 않는다.
- 인계: CSRG-C06과 함께 전체 buffer/replay/수명 상한을 만족할 때만 분리 경로를 활성화한다.

### CSRG-C06 — 진행 중 replay·상한·수명 이벤트

- 소유/예정 PR: CodeSpace / CSRG-P3. 예정 제목: `feat(runner): bound inflight replay buffers and lifecycle delivery`.
- 문제 → 동작: 완료 응답만 캐시하는 replay를 진행 중 attempt까지 확장하고 대기/응답 누적 byte·보존 시간을 제한한다.
- 선행: CSRG-C05. request/attempt별 single-flight ownership, control lane 여유, 완료 출력 보존 정책.
- 대상/산출물: wire replay registry·bounded stdin/output writer, lifecycle 이벤트 및 workspace release callback.
- 불변 조건: 진행 중 중복 dispatch 0; 256KiB 출력 ring 등 기존 사용자 계약 보존; cache evict가 새 실행 허용 아님; 자원 회수와 출력 TTL 분리.
- 시험: 정상 replay·종료 이벤트; 느린 reader·큰 응답·버퍼 초과 거절; in-flight retry/완료/evict/disconnect 경쟁과 lease/slot 누수.
- 검증 명령: 현재 upstream replay/process 시험 + 예정 `python3 scripts/qualify-devguard.py replay-bounds`.
- 완료 증거: 모든 큐의 수·byte·TTL 및 memory peak, 동일 결과 재관측, 이벤트 누락 시 대조 경로, 작업 포화 중 control 성공.
- rollback: 신규 실행 차단 후 in-flight 표를 drain, terminal identity 보존. cache 삭제로 attempt tombstone을 대신하지 않는다.
- 인계: CSRG-C07이 검사할 transport·수명 불변 조건과 최악 부하 fixture를 전달한다.

### CSRG-C07 — 실행 모드 동등성과 장애

- 소유/예정 PR: CodeSpace / CSRG-P4. 예정 제목: `test(resources): exercise pipe PTY and Runner failure parity`.
- 문제 → 동작: 한 경로 성공을 전체 지원으로 확대하지 않고 pipe/PTY × InProcess/UDS의 동일 의미를 입증한다.
- 선행: CSRG-C06. 실제 macOS authority/helper, 각 mode 격리 fixture, 기존 disconnect 동작 expected matrix.
- 대상/산출물: runner/server/PTY integration tests, fault injector, 자격 FD payload 검사와 mode별 결과.
- 불변 조건: 권한·workspace FIFO·approval·resize·exit 계약 유지; DevGuard 장애 중 기존 조회/종료는 handle로 처리; unknown 자동 재실행 0.
- 시험: 4조합 정상; helper/daemon 실패·unsupported requirement·자손 잔류; 승인 resume/취소/replay/종료 경쟁, 기존 모드 연결 소실 회귀.
- 검증 명령: 현재 upstream `integration macos-core`는 해당 platform에서 구분 실행 + 예정 `python3 scripts/qualify-devguard.py parity`.
- 완료 증거: mode별 실행 사례 수, failure 결과·FD 누출 0·중복 0, 미지원/미실행 조합을 skip 이유와 함께 기록.
- rollback: 실패한 mode는 지원표에서 제외하고 required 활성화를 막는다. 전체 제품 합격으로 축약하지 않는다.
- 인계: CSRG-C08에 검증된 runtime mode/fixture/pin 조합만 전달한다.

### CSRG-C08 — upstream 회귀와 관제 SLO

- 소유/예정 PR: CodeSpace / CSRG-P4. 예정 제목: `test(qualification): qualify the pinned DevGuard consumer combination`.
- 문제 → 동작: 독립 DG-1 합격과 실제 CodeSpace 승인·replay·관제 경로 합격을 별도의 제품 증거로 남긴다.
- 선행: CSRG-C07와 DG1-C12. 정확한 source/client/artifact/wire/정책·host, foreground 및 local MCP 측정 fixture.
- 대상/산출물: 제품 qualification report, existing upstream 전체 regression evidence, operator 도입/복귀 문서.
- 불변 조건: 기존 Codex pin/검사 유지; idle10분/부하30분 이상/3회; 원격 RTT 분리; 임의 크기 파일 보호는 claim 범위에서 제외.
- 시험: 정상 개발 workload·MCP 관측; daemon 장애/압력/큰 출력/느린 stdin; 포화 중 replay·승인·종료 경쟁과 foreground SLO.
- 검증 명령: 현재 `validate-upstream.py all`, 별도 `macos-core dependencies` 및 예정 `python3 scripts/qualify-devguard.py codespace-macos`; Linux stage는 실제 Linux에서 별도 실행.
- 완료 증거: 모든 요구 SLO와 raw samples, exact head별 upstream 결과, 지원 조합 manifest. macOS 통과를 Linux 강제 보호로 표시하지 않는다.
- rollback: 검증 조합의 신규 required 소비를 중지·drain 후 이전 조합으로 복귀; 실패 보고서와 pin 후보 보존.
- 인계: P1R-C01과 DGL-C01. 이 단계 완료만으로 Gateway 복구 capability를 선언하지 않는다.
