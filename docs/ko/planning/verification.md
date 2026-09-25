# 검증 범위·합격 기준·증거 보존

승인 설계 [§5.1](../../design.ko.md)의 SLO와 반복 조건을 유지한다. 계획 승인, 구현 완료, fake 계약 통과, 실제 OS 적용, 제품 결합, foreground 응답성은 서로 다른 증거다. 문서 PR의 DG-0 회귀 통과를 새 runtime qualification으로 표시하지 않는다.

## 검증 범위와 소유자

| 범위 ID | 대상과 소유 | 합격 판단 | 현재 제공 여부 |
| --- | --- | --- | --- |
| V-DOC-DG | 이번 DevGuard 문서/메타데이터 | checksum·license·ID·DAG·링크·46작업/23묶음·필수 항목·상태 보존 | 문서 검토 및 아래 재현 검사 가능 |
| V-DG0 | contract/core, DevGuard | Rust1.95.0 fmt/clippy·44개 계약·의존 graph·source fingerprint | 기존 validator 제공 |
| V-DG1-FUNCTION | 실제 auth/probe/launch/reconcile/CLI/운영 | DG1-C01~C11 정상·실패·경쟁 및 기능 artifact | C01 authority·C02 로컬 인증/transport·C03 native probe·C04 native scope 제공; 후속 범위는 각 묶음에서 제공 |
| V-DG1-SLO | 독립 CLI/daemon·개발·self-use | DG1-C12 개발/foreground 및 standalone control 측정 | Harness 제공(`dg1-macos`, `scripts/measure.py macos`); 대상 호스트 측정 대기; CS-RG 기능을 선행 요구하지 않음 |
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
python3 scripts/qualify.py dg1-probes --offline
python3 scripts/qualify.py dg1-scopes --offline
python3 scripts/qualify.py dg1-launch --offline
python3 scripts/qualify.py dg1-reconcile --offline
python3 scripts/qualify.py dg1-cli --offline
python3 scripts/qualify.py dg1-cargo --offline
python3 scripts/qualify.py dg1-bootstrap --offline
python3 scripts/qualify.py dg1-self-use --offline
python3 scripts/qualify.py dg1-upgrade --offline
python3 scripts/qualify.py dg1-macos --offline
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

scripts/qualify.py dg1-authority --offline은 C01 설정·저장소, scripts/qualify.py dg1-auth --offline은 C02 인증·transport, scripts/qualify.py dg1-probes --offline은 C03 native 증거, scripts/qualify.py dg1-scopes --offline은 C04 정책·scope 증거, scripts/qualify.py dg1-launch --offline은 C05 launch helper, scripts/qualify.py dg1-reconcile --offline은 C06 대조, scripts/qualify.py dg1-cli --offline은 C07 명령행 owner, scripts/qualify.py dg1-cargo --offline은 C08 Cargo adapter, scripts/qualify.py dg1-bootstrap --offline은 C09 설치, scripts/qualify.py dg1-self-use --offline은 C10 부모 lease와 후보 authority, scripts/qualify.py dg1-upgrade --offline은 C11 upgrade와 repair, scripts/qualify.py dg1-macos --offline은 C12 SLO harness 검증을 제공하며, 그 protocol은 scripts/measure.py macos이다. 그 밖의 DevGuard suite와 CodeSpace scripts/qualify-devguard.py는 해당 작업에서 제공할 예정 인터페이스다. 각 구현 PR이 실제 CLI·case inventory·nonzero case assertion·timeout·log 수집·격리 cleanup을 구현하고 제공 명령을 갱신해야 한다. macOS/Ubuntu CI는 전체 validator와 이식 가능한 두 기능 suite를 실행하고 각각의 report·log를 보존한다. Native suite는 macOS CI에서만 실행하고 그 밖의 환경에서는 `not_run`으로 기록한다. 증거를 제공할 수 없는 플랫폼에서 통과로 처리하지 않는다. 각 native 단계는 만들어야 할 raw receipt를 선언한다. 어떤 경우를 `not_run`으로 기록한 receipt가 있으면 suite는 `passed`가 아니라 `incomplete`가 된다.

C03 증거는 다음 파일에 기록한다.
- **Raw receipt** (보고서의 `raw/` 디렉터리): 단위를 포함한 boot ID·시계 읽기, 호스트 용량, zombie·reap·거부 관측을 포함한 반복 프로세스 정체성, 계산된 비율을 포함한 native 압력 읽기, 마지막 sample 이후 admission이 닫히기까지 측정한 시간, 주입한 실패부터 Critical까지 걸린 서비스 loop 시간과 멈춘 probe에 대한 서비스 loop의 동작.
- **단계 로그**: 주입한 실패, 거절된 stale·미래·replay·다른 boot sample, 관측한 socket peer의 native 등록.

실제 호스트 압력이 정당하게 다를 수 있는 시험은 합성 정상 읽기를 사용한다. 호스트가 Normal이라고 가정하지 않는다. 이 suite는 wire 등록, scope binding, 정책 적용을 시험하지 않는다.

C04 증거는 다음을 포함한다.
- **Raw receipt**: clamp가 있는 root와 없는 root의 수립 readback(`pbi_nice`, task 우선순위, thread 최대 우선순위), 요청·계획·적용 capability 행렬, 수명주기 timeline, 종료 receipt, 고정된 Suspect 결과를 동반한 이탈 정체성, 공유되거나 비어 있지 않은 group·다른 사용자의 프로세스·kernel 요구·알 수 없는 scope의 거절.
- **실제 프로세스**: 수명주기는 실행 중인 root, reap되지 않은 root, 자손이 남은 채 reap된 root, 정체성을 확인한 종료를 거치며 모든 구성원이 끝난 뒤에만 회수한다.
- **Scripted-table 단위 시험**: 실제 프로세스로 결정적으로 만들 수 없는 경쟁. 관측 중 생성된 구성원, 재사용된 구성원·부모 PID, readback 중 교체된 root, group을 나열하는 동안 교체된 root, 부모 관계를 확인할 수 없는 자식, scope 종료 뒤 재사용된 group ID, scope의 것임을 입증할 수 없는 group, 거부되거나 실패한 읽기, 실패한 signal 전달을 다룬다.
- **Clamp된 환경**: 자가 적용처럼 환경이 harness의 모든 자식에 clamp를 걸면 clamp 없는 root를 만들 수 없다. 이 경우 두 readback과 함께 `not_run`으로 기록하며 suite는 `passed`가 아니라 `incomplete`를 보고한다. Hosted macOS 14 CI runner가 바로 이런 환경으로, harness의 task·thread 우선순위가 20으로 읽힌다. 그래서 CI는 `dg1-scopes`를 `--allow-incomplete`로 실행하고 요약에 `incomplete`를 표시한다. Clamp 없는 경우는 로컬 qualification 호스트에서 통과해야 한다.

서비스는 P3 launch helper 전에는 scope를 수립하지 않으므로 이 결과는 관리 실행이 아니라 라이브러리 증거를 입증한다.

C05 증거는 다음을 포함한다.
- **Raw receipt**: launch 수명주기(단계, READY까지 걸린 시간, 종료 상태, authorize된 attempt와 scope 종료를 통한 회수), executable의 descriptor·인자·디렉터리·환경 변수 이름(값은 기록하지 않음), 하나의 grant에 대한 경쟁 helper와 늦은 helper, claim 전 거절, claim 전 취소, READY 뒤의 exec 실패, bare helper의 replay와 claim 뒤 거절, 기한을 넘긴 응답, 여러 thread의 동시 launch, pseudo-terminal 위의 helper. Receipt에 환경의 자격 값이 없는지 확인한다.
- **실제 프로세스**: 등록된 owner인 시험 프로세스가 실제 `devguard-launch` binary를 시작하고, helper는 같은 프로세스에서 합성 정상 probe로 동작하는 격리 authority에 grant를 제시한다.
- **단위 시험**: core claim transition, owner 확인 중 재사용된 PID를 포함한 scripted helper 정체성 검사, 엄격한 transcript 해석, 엄격한 launch wire fixture, 다른 요청을 할 수 없는 helper session.
- **Clamp된 환경**: claim 뒤에 거절된 helper 경우에는 utility clamp가 없는 helper가 필요하다. Harness의 모든 자식에 clamp가 걸리는 환경에서는 이 경우를 `not_run`으로 기록하고 suite는 `incomplete`를 보고한다. Hosted macOS 14 runner가 그런 환경이며 CI는 `dg1-launch`를 `--allow-incomplete`로 실행한다.

C05만 적용된 정상 서비스는 등록과 launch를 닫아 두었으며, C06이 대조와 함께 이를 연다.

C06 증거는 다음을 포함한다.
- **Raw receipt**: Prepared 취소와 만료, 각 owner 보고에 따른 `NoHelperCreated` 회수와 거절된 늦은 helper, claim된 grant를 회수하지 못하는 보고, 살아남은 구성원이 있는 동안 과금을 유지하는 reap 전 관측, 관측 전에 reap된 root, 알려진 이탈, scope 종료, authorization 뒤의 취소, 죽은 owner, 폐기된 instance, Prepared 기한이 지난 응답 없는 helper, crash 전후에 과금 합계를 측정한 daemon crash와 재시작, bind와 회수의 journal 쓰기 실패.
- **실제 프로세스**: 격리 authority 아래의 실제 helper와 workload. 그중 하나는 시험이 같은 journal에서 종료하고 다시 시작하는 자식 프로세스에서 동작한다.
- **단위 시험**: 추적 상실 뒤 bind된 scope의 이전 boot 회수, 바뀐 것이 없을 때 쓰지 않는 대조, attempt·instance 목록, 크기가 제한된 보고 집합과 owner에 묶인 launcher 증거, 새 요청의 엄격한 decoding.
- **결정성**: 관측 시험은 배경 reconciler를 멈추므로, owner의 관측 또는 reap 전 관측의 부재가 무엇을 추적할지를 정한다. 자식 프로세스 authority의 receipt는 보관하며, permit이나 호출자 자격이 들어 있지 않은지 확인한다.

이 suite는 실행 중인 scope의 재시작 후 재편입과 Linux 강제를 시험하지 않는다.

C07 증거는 다음을 포함한다.
- **Raw receipt**: 인자·디렉터리·환경 변수 이름·descriptor가 보존된 관리 명령의 수명과 그 receipt, CLI가 그대로 반영한 workload의 signal 종료, workload group의 모든 구성원에 전달된 SIGTERM·SIGHUP, 호출자가 무시해 계속 무시되는 signal, 반영하지 않은 SIGSTOP, 살아남은 구성원이 끝날 때까지 과금을 유지하는 reap 전 관측, 거절된 예산과 호스트 작업 용량을 넘는 대기, admission된 대기·기한에 끝난 대기·signal로 취소된 대기, 사용할 수 없는 authority와 fenced launch가 없는 authority, doctor 진단, 프로젝트 해석, owner 8개의 instance pool과 이를 소진하지 않는 순차 owner, 시작할 수 없는 프로그램, pseudo-terminal에서 출력만 terminal에 있는 경우를 포함해 workload에 도달한 interrupt 키와 job control shell에 반영된 정지. Receipt에 호출자 자격이나 상속된 값이 없는지 확인한다.
- **실제 프로세스**: 시험 바이너리를 CLI owner로 다시 실행해 격리 authority에 연결하며, 실제 `devguard-launch`와 workload를 시작한다. Terminal 경우에는 pseudo-terminal session과 최소한의 job control shell을 사용한다. 배포되는 바이너리는 authority override를 받지 않으므로 그 진입점은 사용법과 잘못된 호출만 시험한다.
- **단위 시험**: 엄격한 인자 parsing, 프로그램 탐색, 예산 우선순위, 의미 digest, 끝까지 읽은 transcript, 그리고 scripted authority를 통한 유실된 admission·launch commit 응답, 대기의 재시도 간격·기한·취소, READY 전에 끝난 helper 뒤의 재시도 규칙. Commit된 grant는 받지 못한 것으로 회수되고 다시 만들어지지 않으며, 확인할 수 없는 grant는 회수하지도 다시 시도하지도 않는다.

C08 증거는 다음을 포함한다.
- **Raw receipt**: 예약된 job 수 안의 direct build, 조정되거나 유지된 명시적 job 수, admission 전 거절, 시험 프로그램의 thread 설정을 포함한 `cargo test`, 예약 옆에 최대 동시 수와 실행 뒤 token 수를 기록한 Python pipeline의 공유 jobserver, build script 안의 중첩 Cargo, 관측한 최소 여유 token 수를 포함한 상속 FIFO·descriptor 쌍 jobserver, 오래된 상속 descriptor, 최대 과금을 포함한 동시 소비자, 취소된 pipeline. 각 receipt는 관측한 최대 동시 compile 수와 각 Cargo가 받은 값을 기록한다.
- **실제 프로세스**: CLI owner와 실제 helper를 통한 작은 offline workspace의 실제 Cargo build. Compiler wrapper가 각 compile의 시간을 재고, `cargo` shim이 각 실행이 상속한 descriptor를 기록한다. 호스트의 작업 용량이 Cargo job을 수용할 수 없는 경우는 `not_run`으로 기록한다.
- **단위 시험**: job 추정, subcommand와 `--` 전후의 인자 parsing, Cargo가 읽는 방식대로 해석한 job 값, jobserver 아래를 포함한 우선순위·조정·거절, fallback 변수, 닫혔거나 close-on-exec인 참조·descriptor 쌍·FIFO 참조에 대한 상속 jobserver 검사, FIFO pool과 그 크기 상한·token 수, adapter 선택.

C09 증거는 다음을 포함한다.
- **Raw receipt**: 실행 중인 PID·실행 파일·hash를 포함한 검증된 설치, 재사용한 같은 release와 거절한 다른 release, 거절된 패키지, authority 제공 중이거나 상태가 없을 때의 거절, 아무것도 선택하지 않고 unload된 미검증 서비스, 동시 installer, crash 재시작·정지·닫힌 채 끝나는 시작을 포함한 launchd 수명 주기.
- **실제 프로세스**: 각 패키지의 `devguardd`로 복사한 시험 바이너리가 격리된 fixture authority를 제공한다. Fake manager는 launchd와 같은 방식으로 이를 시작한다. Native 경우는 launchd 자체를 사용한다. 고유한 임시 label과 시험 디렉터리 아래의 plist를 쓰며, `~/Library/LaunchAgents`는 쓰지 않고, 모든 경로에서 bootout한다.
- **단위 시험**: `launchctl print` parsing, escape와 crash 전용 재시작을 포함한 plist 생성, release id 규칙, 이 build에 컴파일된 호환성.
- **실제 설치**: 사용자 확인이 필요한 qualification 호스트의 설치는 패키지 manifest, status 보고, 실행 중인 바이너리 hash와 함께 C09 완료 증거로 기록한다.

C10 증거는 다음을 포함한다.
- **Raw receipt**: workload와 후보 authority를 자식으로 실행하고 해제로 예산을 돌려준 lease, 제공 전에 죽은 후보, 제공하지도 닫히지도 않아 멈춘 후보에 대한 `test-candidate` 보고, lease 안·밖·종료 뒤의 lease 자식과 위조 token을 쓴 자식. 실행이 남긴 파일에 caller 자격이 없고, lease 자식의 receipt에 lease token이 없는지 확인한다.
- **실제 프로세스**: 시험 바이너리를 `devguard exec --lease` owner, 실제 `devguard-launch` 아래의 workload, 격리 fixture 부모의 lease를 쓰는 후보 authority로 다시 실행한다. `test-candidate` orchestrator는 lease owner로서 시험 프로세스 안에서 실행한다.
- **단위·서비스 시험**: core lease 계약(과금, 남은 예산, token, 재요청, 종료·owner 소실·기한에 의한 fence, Suspect 자식, 재시작, 폐기, 새 table이 없는 journal), 요구한 client에게만 알리는 capability, token만 쓰는 보유자 session, 후보 session과 그 거절, 엄격한 lease wire fixture, 인자 parsing, 후보 tree로 만드는 plan.
- **실제 자기 적용**: 동결해 설치한 C10 부모 아래에서 작업 tree로 실행한 `test-candidate`는 그 release에 대한 사용자 확인이 필요하며, 보고·receipt·부모 hash와 함께 C10 완료 증거로 기록한다. 호스트 메모리 압력이 admission을 허용해야 한다.

C11 증거는 다음을 포함한다.
- **Raw receipt**: drain·백업·닫힌 시작·재개를 거친 정상 upgrade, 현재 release와 그 과금을 유지한 drain 시간 초과와 작업 정리 뒤의 같은 upgrade, 시작할 수 없던 release와 다시 제공하는 이전 release, 거절된 호환되지 않는 downgrade, 과금된 작업이 있는 동안 거절된 drain 불가 release의 정지 교체, upgrade가 교체한 release로 되돌아가는 repair, 복구 사본을 쓴 repair, 열 수 없는 journal에서의 repair, 중단된 upgrade가 닫아 둔 admission을 다시 여는 repair, 복구 사본으로 repair한 release에서의 upgrade, 실패한 멈추기, 검증 중에 죽은 release, signal로 취소된 drain, 다시 실행해 완료하는 중단된 upgrade, repair가 완료하는 중단된 repair, 서비스를 기록할 수 없어 다시 unload된 설치.
- **실제 프로세스**: 각 패키지의 `devguardd`와 `devguard`로 시험 바이너리를 복사하므로, 각 upgrade는 실제로 자신이 설치하는 release에서 실행된다. 각 서비스는 격리 fixture authority를 제공하는 그 사본이며, fake manager가 launchd와 같은 방식으로 시작한다.
- **단위·서비스 시험**: core quiescence 계약(Prepared attempt만 취소, 활성화 없이 읽는 유휴 journal의 과금, 완전한 journal인 백업, lease table이 없는 journal), 재시작 뒤에도 유지되는 관리자 drain 요청, 엄격한 drain wire fixture, 해제되지 않은 lease를 셀 수 없는 last known good release를 포함한 호환성 규칙, release id, 인자 parsing.
- **실제 upgrade**: 사용자 확인이 필요한 qualification 호스트의 설치 release upgrade는 stage·upgrade 보고, 백업 hash, status와 함께 C11 완료 증거로 기록한다.

C12 증거는 다음을 포함한다.
- **Raw receipt**: 제어 probe가 자기 target에서 얻은 상태·종료 표본을 담는다. 종료 표본은 확인된 뒤 scope 종료로 해제된다. 중지되어도 target을 정리하는 경우와, admit되지 않아 아무것도 과금하지 않는 target도 포함한다. 표본은 도착하지만 결코 유효한 관측이 아닌 headless fixture의 보고도 포함하며, Chrome이 없으면 이를 `not_run`으로 기록한다.
- **단위 시험**: 제한된 작업 부하와 protocol 규칙을 검사한다. 규칙은 누락 표본을 모든 값보다 위에 두는 nearest-rank 백분위수, 페이지 추정치를 올리는 Event Timing duration, frame 정지, 연결 유실, 실제 적용된 부하, 유효성, 그리고 구간·반복·조합 판정이다.
- **측정**: 사용자가 확인한 시간에 qualification 호스트의 설치 release에 대해 `scripts/measure.py macos`를 실행한다. Source·artifact·정책·환경·harness hash를 담은 run header를 기록한다. 반복마다 원시 표본·receipt·보고를 보존하고, 모든 판정을 담은 요약을 쓴다. Rehearsal이나 headless 실행은 inconclusive로 기록한다. Qualified 실행만 승격하며, 승격 기록은 서비스가 그 release를 실행한다는 검증과 함께 보존한다.

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
