# 검증 범위·합격 기준·증거 보존

승인 설계 [§5.1](../../design.ko.md)의 SLO와 반복 조건을 유지한다. 계획 승인, 구현 완료, fake 계약 통과, 실제 OS 적용, 제품 결합, foreground 응답성은 서로 다른 증거다. 문서 PR의 DG-0 회귀 통과를 새 runtime qualification으로 표시하지 않는다.

## 검증 범위와 소유자

| 범위 ID | 대상과 소유 | 합격 판단 | 현재 제공 여부 |
| --- | --- | --- | --- |
| V-DOC-DG | 이번 DevGuard 문서/메타데이터 | checksum·license·ID·DAG·링크·46작업/23묶음·필수 항목·상태 보존 | 문서 검토 및 아래 재현 검사 가능 |
| V-DG0 | contract/core, DevGuard | Rust1.95.0 fmt/clippy·44개 계약·의존 graph·source fingerprint | 기존 validator 제공 |
| V-DG1-FUNCTION | 실제 auth/probe/launch/reconcile/CLI/운영 | DG1-C01~C11 정상·실패·경쟁 및 기능 artifact | C01 authority·C02 로컬 인증/transport 제공; 후속 범위는 각 묶음에서 제공 |
| V-DG1-SLO | 독립 CLI/daemon·개발·self-use | DG1-C12 개발/foreground 및 standalone control 측정 | 미구현; CS-RG 기능을 선행 요구하지 않음 |
| V-CS-DOC | CodeSpace 한·영 registry/site | paired hash·기존 docs tests·고정 환경 build·integrity·화면 검토 | 기존 명령 제공 |
| V-CS-UPSTREAM | CodeSpace 기존 Codex qualification | pin/policy/format/dependency/adapter/PTY/filesystem/platform gates | 기존 제공, 실제 platform별 수행 |
| V-CS-RG | CodeSpace 결합 | CSRG-C07/C08의 모드 동등성·승인·replay·관제 포화 SLO | 미구현 |
| V-P1 | 독립 Runner+Gateway 재시작 | P1R-C06의 동일 실행·fence·timeout·출력/unknown | 미구현 |
| V-LINUX | 실제 Linux scope와 제품 조합 | DGL-C05/C06 controller·ancestor·권한·자손·SLO | 미구현; fake cgroup과 분리 |
| V-CACHE / V-ADAPTER | cache/tool/executor | DGC-C06, DGA-C02/C04/C06/C08의 해당 조합 | 미구현 |

상태는 `passed`, `failed`, `not_run`, `inconclusive`를 구분한다. 기존 도구가 `incomplete`를 출력하면 그 값을 보존하고 어떤 필수 범위가 미완료인지 덧붙인다. 실행하지 않은 시험과 환경 부적합을 통과로 바꾸지 않는다. 새 suite는 발견/실행 case 수가 0일 때 실패해야 한다.

## 현재 재현 가능한 명령

DevGuard root, Rust **1.95.0**(rustfmt/Clippy 포함), Python 3.11 이상:

```sh
python3 scripts/check_docs.py
python3 scripts/validate.py --offline
python3 scripts/qualify.py dg1-authority --offline
python3 scripts/qualify.py dg1-auth --offline
git diff --check
```

locked dependency가 없으면 offline을 제거한다. `--output`은 존재하지 않는 ignored repo 내부 경로만 사용한다. 정상 PATH가 다른 Rust면 설치된 1.95.0 toolchain을 먼저 둔다. mismatch 허용 결과는 supplemental이며 지정 toolchain qualification이 아니다. 이 검사는 OS/foreground/self-use를 `not_run`으로 남긴다.

CodeSpace root, Node **24.21.0**, npm **11.19.0**, Python 3.11 이상(CI 3.14):

```sh
python3 -B scripts/check_docs.py
npm ci --prefix docs-site --ignore-scripts
npm test --prefix docs-site
npm run build --prefix docs-site
python3 -B docs-site/scripts/site.py check
```

`DOCS_PYTHON`으로 Python을 명시할 수 있다. 번역 pair를 실제 대조한 뒤에만 `python3 -B scripts/check_docs.py record --id devguard-integration`으로 해당 pair의 hash를 갱신한다. 전체 registry를 일괄 재기록하지 않는다. 사이트 source Markdown·언어 이동·긴 표·desktop/narrow·light/dark를 확인한다. 문서 PR 병합 후 별도 main CI와 CodeSpace 문서 배포까지 확인해야 준비 단계가 완료된다.

현재 upstream runtime gate는 다음과 같다. **이번 문서 변경에서는 기존 CI가 이를 실행하며, 아래 명령 존재를 새 DevGuard 기능 구현으로 해석하지 않는다.**

```sh
python3 scripts/validate-upstream.py all
python3 scripts/validate-upstream.py macos-core dependencies
python3 scripts/validate-upstream.py linux-isolation
```

첫 명령은 macos-core를 포함하지 않는다. macOS에서 Linux isolation skip/incomplete는 실제 Linux 검증을 대체하지 않는다. Linux controller와 sandbox 권한이 있는 환경의 결과를 별도 확인한다. target은 기존 `target/upstream-validation`, 보호 보고서는 `target/upstream-reports/local` 규칙을 유지한다.

## 이번 문서의 정합성 검사

V-DOC-DG는 scripts/check_docs.py와 수동 의미 검토로 영어/한국어 hash·작업·링크를 검사하며 기존 runtime 검증을 보존한다. 실행 로그와 결과를 ignored evidence에 보존한다.

1. `docs/design-source.json`의 SHA-256을 실제 설계 byte와 비교하고 기준 commit의 LICENSE/NOTICE·runtime·Cargo·workflow를 대조하고 validator의 기존 gate가 제거되지 않았는지 확인한다.
2. 7개 milestone 파일의 `### PREFIX-Cnn` 정의가 DG1 12, CSRG 8, P1R 6, DGL 6, DGC 6, DGA 8로 총46개인지 확인한다. DG0-R 이행 기록과 이번 DGP/CSP 문서 commit을 제외한다.
3. PR table의 고유 계획 ID가 6+4+3+3+3+4=23개인지, 각 작업이 한 묶음에 속하는지 확인한다. 각 `선행` 필드와 milestone ledger graph를 추출해 정의되지 않은 참조와 cycle을 거절한다.
4. 작업마다 소유/예정 제목·문제/동작·선행·대상/산출물·불변 조건·정상/실패/경쟁 시험·명령 제공 상태·완료 증거·rollback·인계가 있는지 확인하고 내용의 구체성은 사람이 다시 읽는다.
5. Markdown 로컬 링크와 `milestones.json` 문서 경로, 기존 영어 진입점, 외부 고정 source 링크를 확인한다. 미래 SHA/PR 번호와 현재 없는 CLI를 실행 지침처럼 쓰지 않는다.
6. `milestones.json`의 기존 상태/선행/critical_path를 원본과 비교한다. 정본 design/document 참조 외의 기존 상태·선행은 보존한다.
7. CodeSpace runtime/Codex gitlink와 원래 checkout의 staged `.codex/config.toml` blob을 작업 전후 비교한다. canvas는 계획/PR 참조만 연결하며 후속 구현은 미착수다.

## 향후 suite와 장애 주입 계약

scripts/qualify.py dg1-authority --offline은 C01 설정·저장소, scripts/qualify.py dg1-auth --offline은 C02 인증·transport 검증을 제공한다. 그 밖의 DevGuard suite와 CodeSpace scripts/qualify-devguard.py는 해당 작업에서 제공할 예정 인터페이스다. 각 구현 PR이 실제 CLI·case inventory·nonzero case assertion·timeout·log 수집·격리 cleanup을 구현하고 제공 명령을 갱신해야 한다. macOS/Ubuntu CI는 전체 validator를 유지하며 두 기능 suite도 실행하고 각각의 report·log를 보존한다.

C02 증거는 양쪽 실제 OS socket UID/PID 관측, 분리된 consumer·관리 자격, helper-role 인증 거절, 현재·미래 wire fixture의 엄격한 decoding, 64 KiB frame, 세션 32개 제한, idle 대기를 포함한 frame별 절대 250 ms 기한, 부분·느린·마지막 응답과 private 자격 FD 전달을 포함한다. 전용 subprocess helper는 후속 exec 전 FD 닫기를 관측하고 argv·환경·debug·출력의 secret 누출을 확인한다. Parent test가 helper를 실행하며 별도의 ignored test를 독립 qualification 성공으로 세지 않는다. 0개가 아닌 parent case 수와 프로세스 정리를 함께 기록한다.

Framing은 poll·descriptor O_NONBLOCK·호출별 nonblocking I/O로 Darwin timeout 옵션 변경 없이 peer 종료 뒤 버퍼 데이터를 보존한다. 느린 writer와 느린 reader를 모두 시험한다. 인증과 닫힌 등록 응답은 boot/start 정체성, native 등록/Principal, lease, OS 정책, helper 권한·launch를 입증하지 않으며 해당 범위는 P2/P3까지 미검증이다. system_tasks >= 48도 검증된 회계 추정치이지 kernel task 제한이나 측정된 충분성이 아니다. 같은 schema 1에서 이전 값 16을 거절하는 시험을 명시하며 자동 migration을 의미하지 않는다.

| 시험 영역 | 주입 지점/반드시 보존할 불변 조건 | 담당 작업 |
| --- | --- | --- |
| authority/auth | 경로 alias·이중 시작·잘못된 UID/PID/generation·credential 누출 | DG1-C01/C02, CSRG-C02 |
| 회계/내구성 | admission/commit 응답 유실·같은 키 변경·journal write 실패·restart | DG1-C05/C06, CSRG-C03/C04 |
| launch | helper 생성 전/READY 전/READY 후 실패, cancel/expiry/late helper | DG1-C05/C06, CSRG-C07 |
| 수명 | root 종료/자손 잔류·PID reuse·tracking loss·원래 boot deadline | DG1-C03/C04/C06, DGL-C04 |
| 관제 | 큐·누적 byte 포화·느린 stdin/reader·shared lock/callback·동시 replay | CSRG-C05~C08 |
| 자기 적용/upgrade | 후보 crash·과도 budget·parent 소실·정책/journal 실패·구신 strict decoding | DG1-C09~C12 |
| Gateway 복구 | 동시 Gateway·stale epoch·종료/reconnect·Runner loss·출력 gap | P1R-C01~C06 |
| Linux | 실제 controller/ancestor/권한·sandbox/proxy·OOM·자손 회수 | DGL-C01~C06 |
| cache | use/reclaim 경합·root 교체·rename/sweep crash·trash 회계 | DGC-C01~C06 |
| adapter/executor | 옵션 충돌·중첩 token/FD·자식 budget·CLI와 executor 수명 차이 | DGA-C01~C08 |

실패 주입은 시험 전용 scope/root와 명시 budget에서 수행한다. 최초 R1은 제한된 기능 시험이며, 모든 부하를 일상 호스트에 무제한 주입하는 권한이 아니다. 시험 중단은 신규 작업을 닫고 실제 scope를 대조하며 결과를 실패/불확실 그대로 보존한다.

## SLO와 반복 방법

각 지원 backend/조합에서 **idle 기준선 10분 + 부하 최소30분, 3회 반복**한다. 실제 검증 명령이 30분을 넘으면 완료까지 관측한다. fixed source Cargo build/test, 여러 소비자, 제한된 CPU/memory/I/O, 출력 포화·느린 stdin을 포함한다. cold와 warm을 분리하고 cold는 시험 디렉터리에서 만든다. 운영 cache를 지워 baseline을 만들지 않는다.

| 측정 항목 | 승인된 초기 합격 기준 |
| --- | --- |
| 개발 도구·MCP 연결 | 자원 압력으로 유발된 소실 0회 |
| 로컬 `process_status` | p99 ≤500ms |
| 종료 요청 수락·응답 | p99 ≤1초; 실제 scope 종료 시간 별도 |
| foreground 입력→다음 paint | p99 ≤100ms; 1초 초과 0회 |
| foreground frame 진행 | 500ms 초과 정지 0회 |
| 중복 실행·중복 예약 | 0회 |
| 보호 대상 GC | 0건 |
| 불확실 실행 자동 재시작 | 0회 |

DG-1에서는 standalone CLI/daemon의 대응되는 조회·종료와 개발/foreground를 측정하고 제품 미결합 항목은 해당 없음/미실행으로 명시한다. **실제 CodeSpace MCP의 process_status·terminate_process, 승인/replay/관제 포화는 CSRG-C08에서 추가로 통과해야 한다.** 이런 범위 분리는 SLO 수치를 완화하지 않으며 DG1→CSRG 의존을 순환시키지 않는다.

브라우저는 고정 로컬 foreground fixture의 스크롤/입력/화면 변경과 paint/frame을 측정한다. background timer throttling을 foreground 성능과 혼동하지 않는다. baseline 자체 미달 또는 측정 환경 무효면 `inconclusive`; fixture 합격을 모든 웹사이트 보장으로 확대하지 않는다.

각 실행의 local control 접수→응답 구간과 remote RTT/사용자 체감 시간을 따로 기록한다. p99 계산법·표본수·sampling 간격·clock과 누락 비율을 고정하고 원시 latency를 보존한다. 평균이나 전체 합산 p99 하나로 실패한 반복을 숨기지 않는다.

## 증거 묶음과 승격

필수 manifest는 양 저장소 source HEAD·dirty fingerprint, 실제 daemon/helper hashes·client pin·wire/capability, policy revision/journal schema, host 모델·RAM·OS/kernel·arch·전원·boot/clock, controller/ancestor/권한, fixture revision·cache 상태·명령·시각을 가진다. 값이 없으면 unknown을 표시하고 지원 claim을 제한한다.

raw pressure/latency/jobs 전환, peak memory, 완료 시간·처리량, 거절 이유, 큐/버퍼 peak, attempt/slot/lease lifecycle, 장애 지점, 실제 종료/readback을 저장한다. report·로그·원시 파일 manifest/hash를 함께 보존하고 token/credential/사용자 payload를 정제한다. source와 evidence를 같은 의미로 취급하지 않는다.

qualification은 정확한 artifact·정책·환경 조합에 귀속한다. documentation-only head에서 계약 회귀가 통과했다고 기존 binary의 SLO를 새로 측정한 것처럼 표시하지 않는다. CI run URL·job·event·head·artifact 이름을 PR에 연결하고 PR head 검사와 merge/push-main 검사를 구분한다. 이번 승인 범위는 PR 검사·정상 병합·별도 main 검사와 증거 보존/정리까지다.

C10 기능 부모와 실제 자기 적용 receipt를 보존하고 C12 측정 artifact/정책/환경만 승격한다. 현재 대상은 8논리CPU/16GiB macOS다. foreground visibility/focus는 전체 측정 구간에서 검증하며 무효이면 inconclusive다. 정리 전 raw/report/manifest/log를 worktree 밖 보호 경로로 복사하고 hash를 확인한다.
