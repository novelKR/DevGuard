# 문서와 후속 구현 PR 진행서

이번 완료 조건은 상세 문서, 두 저장소의 문서 정합성 검증, 두 문서 PR 제출 및 실제 head의 검사 결과 확인이다. 병합과 후속 runtime 구현은 별도 단계다. 계획 ID는 GitHub PR 번호가 아니며 미래 commit SHA를 미리 부여하지 않는다.

## 이번 문서 PR의 commit 단위

| 작업 ID | 저장소/예정 branch | 예정 commit 범위 | 검증·인계 |
| --- | --- | --- | --- |
| DGP-D01 | DevGuard `codex/planning-documents` | 기준 source·등록/복구 결정·원문 관계 | source 대조, 승인 원문 보존 → D02 |
| DGP-D02 | 같은 branch | 7개 milestone, 46개 작업·23개 묶음·시험·rollback | ID/PR 배정/선행·필수 항목 → D03 |
| DGP-D03 | 같은 branch | 공통 소비 조건·CodeSpace mapping·검증·이 PR 진행서 | 현재/미래 경계·명령·SLO 검토 → D04 |
| DGP-D04 | 같은 branch | 계획 index, 영어 README/로드맵, ledger 문서 참조 | cross-link·상태 보존·DG-0 exact-source 검증 → DevGuard PR |
| CSP-D01 | CodeSpace `codex/devguard-planning-links` | 한·영 최소 도입 조건·단일 Runner·최초 복구 범위 | pair 대조·선택 registry hash → D02 |
| CSP-D02 | 같은 branch | DevGuard 문서 revision·PR 연결과 번역 registry | 고정 링크·site build/integrity → CodeSpace PR |

각 문서 commit은 예정 제목에 ID를 남겨 실제 history에서 찾을 수 있게 한다. 작성 중 문서 link가 뒤 commit에서 완성되는 경우 최종 branch에서는 모두 존재해야 한다. DGP/CSP는 runtime 46개 단위에 포함하지 않는다. 실제 commit/PR URL은 생성 후 PR body와 인계 보고에 기록하며 이 문서에 가상 번호를 만들지 않는다.

DevGuard는 깨끗한 `main`의 `d59cbd43d206a9a9281328a946eddf1dc199f710`에서 시작한다. CodeSpace는 별도 checkout에서 기존 로컬 로드맵 `fb822fc24c98f6628dce62d33a5cc67275f8ca34`를 기반으로 한다. main runtime 기준은 `e94d21475643608ad2a466256fb57266b86faa47`이다. CodeSpace PR diff에는 미제출 로드맵 commit도 함께 들어간다. 원래 checkout의 staged `.codex/config.toml`은 건드리지 않는다.

## 제출 순서와 검토 가능한 상태

1. 작업 전 branch·HEAD·dirty/index를 기록하고 적용 지침/설계/contracts를 읽는다. DevGuard의 ignored 설정·도구·evidence를 commit에 넣지 않는다.
2. DGP-D01~D04를 작성하고 [V-DOC-DG/V-DG0](verification.md)를 통과한다. 승인 설계 checksum·LICENSE/NOTICE, runtime·Cargo·journal schema·workflow·validator 무변경을 확인한다.
3. 제출 직전 owner/repo/head branch의 기존 open PR을 재조회한다. 있으면 갱신하고 없으면 DevGuard 문서 PR을 먼저 제출한다.
4. 제출한 DevGuard **실제 full head SHA**로 `blob/<SHA>/docs/planning/...` 링크를 만든다. PR이 미병합이면 `main`에 아직 없는 파일을 링크하지 않는다. 문서 reference와 runtime dependency pin을 분리한다.
5. CSP-D01/D02에서 두 언어를 같은 의미로 수정하고 해당 pair registry만 갱신한다. pinned Node/npm의 docs tests/build/integrity 및 문서 화면을 확인한다.
6. CodeSpace의 기존 PR도 다시 확인해 중복을 피하고 main을 base로 문서 PR을 제출한다. DevGuard PR URL과 full document revision, 기존 fb822 포함 사실을 body에 기록한다.
7. 각 PR의 실제 head·파일 범위·검사 rollup과 run event를 확인한다. 기존 CI의 필수 검사를 docs라는 이유로 면제하거나 workflow를 변경하지 않는다. 실패는 관련 문서 문제를 고친 뒤 새 head 결과를 다시 확인한다.
8. canvas에는 실제 상세 문서/두 PR 링크만 연결하고 향후 기능을 미착수로 유지한다. 저장소 문서가 원본이며 canvas는 보조다.
9. 검사 완료 후 두 PR URL·head·결과·로컬 검증·미실행 runtime 범위·원문/설정 보존을 인계한다. PR merge는 수행하지 않는다.

검사 대기 동안 draft로 제출할 수 있다. 검토 준비 상태로 전환하려면 현재 head의 요구 checks가 끝나고 실패·미완료를 사실대로 표시해야 한다. head 수정 전 결과를 새 head 통과로 재사용하지 않는다. 자동 병합은 이 문서의 제출 권한에 포함하지 않는다.

## PR 설명과 증거 항목

PR 설명은 해결하는 문제와 결과 문서의 역할로 시작한다. 이번 DevGuard PR은 구현 가능한 커밋/시험/도입 조건의 부재를, CodeSpace PR은 소비자가 알아야 할 최소 조건과 결정의 부재를 설명한다. 대화 이력 대신 reviewer가 diff만으로 이해할 내용을 쓴다.

| 항목 | 기록 내용 |
| --- | --- |
| 범위 | 문서/메타데이터 변경 경로, runtime 상태가 unchanged임을 확인한 diff |
| 기준 | 실제 base/head, DevGuard d59·CodeSpace e94, 문서 revision과 runtime pin 구분 |
| 계약 | Runner 단일 등록, opt-in Gateway 복구, 기존 mode 수명 보존 |
| 검증 | 실행 명령·toolchain·결과·report 위치, CI run/job/head/event |
| 의존 | 상대 PR URL와 immutable 문서 링크, merge 순서 및 재검증 조건 |
| 한계 | DG-0 fake 범위·새 OS/SLO not_run, 지원하지 않는 환경 |
| 복귀 | 문서 commit revert·링크 재고정; runtime journal 변경 없음 |

원시 로그/secret/개인 설정은 공개 diff에 넣지 않는다. PR body를 CLI로 제출할 때 body file을 사용해 실제 줄바꿈을 보존한다. 생성한 PR을 작업의 artifact로 연결해 사용자가 diff를 열 수 있게 한다.

## 후속 23개 논리 PR 묶음의 운영 규칙

상세 범위의 원본은 [마일스톤 index](README.md)다. DG1 6개 → CSRG 4개 → P1R 3개가 최초 경로다. DGL 3개는 전체 제품 완료에 필수이고 DGC 3개·DGA 4개는 DG1 이후 별도로 진행한다. DGA-P4의 실제 Linux 의존은 해당 작업에 추가로 표시되어 있다.

각 논리 PR은 해당 작업 commit의 시험·계약·취소/정리까지 묶는다. launch만 있고 회수가 없는 기능, control lane만 있고 누적 buffer 상한이 없는 기능, cache rename만 있고 restart sweep이 없는 기능을 운영 활성화하지 않는다. DevGuard와 CodeSpace 양쪽 코드가 필요한 묶음은 연계 PR로 나누되 같은 검증 조합 manifest를 공유한다.

선행 PR 미병합 상태에서 준비할 수는 있으나 실제 pin 선정과 qualification은 정확한 immutable source/artifact로 한다. 선행 head가 바뀌면 영향받는 계약/조합 검증을 다시 수행한다. PR 리뷰에서는 해당 작업의 정상·실패·경쟁 case와 완료 증거를 확인하고 미실행을 숨기지 않는다. Linux·foreground long-run은 unit CI로 대체하지 않는다.

runtime 변경 PR은 schema/version·public API·dependency·운영 rollout의 승인된 범위를 다시 대조한다. 기존 방향 안의 구현 세부는 반복 승인을 요구하지 않지만 Runner 자체 I/O 복구, 다중 하위 등록, 임의 파일 크기 보호를 위한 API 변경 등 새로운 범위는 별도 설계 결정을 받아야 한다.

## rollback과 종료 조건

문서 PR은 merged runtime에 영향을 주지 않으며 미병합 branch 수정 또는 문서 revert로 복귀한다. 이미 다른 저장소에서 고정 링크로 소비한 문서 commit은 force push로 제거하지 않고 새 revision으로 대체한다. 상대 링크를 함께 갱신하고 registry를 다시 검토한다.

future runtime rollback은 각 작업의 절차를 우선한다. 공통으로 신규 admission 차단, live scope 관측/정리, journal/attempt/approval 대조, 호환 artifact 복귀 순서를 지킨다. 설정을 off로 바꾸는 것만으로 charge·자손·입출력이 정리되었다고 판단하지 않는다. 증거와 복구 artifact는 cache가 아니다.

이번 종료 checklist는 13개 상세 문서·기존 진입점/ledger 연결, 46/23 정합성, 원문·license·Codex pin·staged 설정 보존, 양 언어/site 검증, 두 문서 PR과 현재 head CI 결과, canvas 미착수 상태 유지다. 병합·후속 runtime commit·제품 qualification 완료는 포함하지 않는다.
