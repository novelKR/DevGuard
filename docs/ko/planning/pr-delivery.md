# PR 전달·증거·정리

영문 정본은 [PR delivery](../../planning/pr-delivery.md)다. 승인 범위는 문서 준비 후 DG1-C01~C12의 6개 PR을 순차 구현·검토·현재 head 검사·정상 병합·별도 main 검사·정리까지 완료하는 것이다. PR 본문과 정본 문서는 영어이며 한국어 번역의 검토 hash를 유지한다. 계획 PR ID는 미래 GitHub 번호가 아니다.

## 문서 이력과 준비

DGP-D01은 기준/결정,D02는7개 milestone·46작업·23묶음,D03은 소비/결합/검증/전달,D04는 index/README/ledger다. CSP-D01은 이중언어 소비·단일 Runner·복구 범위,D02는 immutable 문서/PR 링크·registry다. CSP-D03은 CodeSpace 결합 로드맵에 DG-1 완료와 CS-RG 미시작 상태를 반영하고([CodeSpace #66](https://github.com/novelKR/CodeSpace/pull/66), merge `a1166870acbba9791d7170da6496a39dde6f4a69`), DGP-D05는 CS-RG 계획·결합 명세·ADR-001·계약을 구현된 DG-1 소비 인터페이스에 맞춘다. 같은 branch의 DGP-D06은 사용자 지시로 채택한 [설계 개정 1](../design-revision-1.md)(실행 소유권·재사용 정책, CSRG-C00/C09, 개정된 계획 문서)을 반영하며 병합 전 사용자 검토를 거친다. CSP-D04는 병합된 DevGuard SHA에 연결하는 CodeSpace 대응 문서(결합 로드맵·architecture·execution substrate·Codex 재사용·upstream 갱신·의존 규칙·CI 선택)이며 작업을 시작할 때 branch를 만들고 CSRG-P0 전에 끝낸다. 이 문서 작업은 runtime48개와 별개다. DevGuard 기준d59cbd43d206a9a9281328a946eddf1dc199f710,CodeSpace runtime e94d21475643608ad2a466256fb57266b86faa47와 로드맵fb822fc24c98f6628dce62d33a5cc67275f8ca34를 보존한다. 원래 CodeSpace checkout·사용자 branch·staged .codex/config.toml은 유지한다.

[DevGuard #1](https://github.com/novelKR/DevGuard/pull/1)에 추가 commit으로 영문 정본·관리 한국어 번역·hash 검사와 C10 지침을 반영한다. 승인 docs/design.ko.md와 checksum 및 기존 commit/link를 바꾸지 않는다. [CodeSpace #65](https://github.com/novelKR/CodeSpace/pull/65)는 실제 전체 DevGuard 문서 SHA로 연결하고 해당 번역 pair만 검토/기록한다. 미병합 main의 없는 경로를 링크하지 않는다. 문서 revision과 runtime pin은 별개다.

DevGuard #1 검증·병합·push-main 확인 후 CodeSpace #65의 검증·병합·runtime main CI·기존 문서 배포를 확인한다. 문서 PR이라는 이유로 기존 gate를 면제하지 않는다. 두 cycle 완료 전 P1을 시작하지 않는다.

## 설계 개정과 문서·코드 PR

설계 개정 1의 근거는 DG-1 완료 뒤 2026-09-27에 사용자가 CS-RG 설계의 재검토를 명시적으로 지시한 것이다. 과거 승인은 무엇을 바꾸는지 알려 줄 뿐 대안을 기각하는 근거가 아니다. DGP-D05/D06 같은 문서 PR은 파일 변경을 문서와 문서 검사에 한정하지만 후속 구현을 구속하는 설계 결정은 바꾼다. 구현이나 qualification을 완료하지 않으며 DG-1 qualification을 무효화하지도 않는다. runtime artifact를 바꾸는 코드 PR은 이전 검증을 새 구현의 검증처럼 제시하지 않는다.

CS-RG의 최종 head는 CSRG-C07 동등성, CSRG-C09 결정(C09가 코드를 바꾸면 영향받는 동등성 재실행), 그 head를 측정하는 CSRG-C08 순서로 검증한다. PR head 결과와 병합 후 main 결과를 따로 기록한다. CSP-D04는 DevGuard 병합 commit이 생긴 뒤에만 그 SHA로 immutable 링크를 기록한다.

## 구현 PR 하나씩 전달

지속 DevGuard checkout은 저장소·보호 증거 위치다. 검증된 main에서 codex/dg1-p1~p6 별도 task worktree를 만든다.

1. base/head/index·이전 병합·main 결과를 기록하고 지침·계약·영향 경로를 읽는다.
2. 해당 묶음 commit·정상/실패/경쟁 시험·영문/한국어 문서만 구현한다. crate 추가 시 명시 allowlist를 늘리되 전체 dependency 검사를 유지한다.
3. 로컬 검사와 전체 diff의 동작·실패·호환·정리 검토를 완료하고 증거를 보존한다.
4. 기존 PR을 재조회해 중복을 피하고 영문 본문으로 제출/갱신한다. 생성 PR은 task에 연결한다.
5. 현재 full head·draft/mergeability/review·예상 전체 checks·저장소 정책을 다시 확인한다. 이전 head 통과를 재사용하거나 검사/리뷰를 우회하지 않는다.
6. 해당 repo에서 gh pr merge <실제번호> --merge --match-head-commit <검증SHA>로 정상 병합한다. admin 우회나 원격 branch 삭제를 하지 않는다.
7. MERGED·merge OID·parent ancestry/tree·fetch한 main을 확인하고 별도 push-main workflow 완료를 기다린다.
8. 증거 복사/hash·실행 중 프로세스 사용 여부를 확인하고 해당 output/worktree/local branch를 정리한 뒤 다음으로 넘어간다.

병합/main 검사 실패는 worktree·증거를 유지하고 원인을 해결할 때까지 다음 PR을 막는다. 기존 승인 내 세부는 반복 확인하지 않되 범위·권한·소유권·호환성·workload 가정·합격 기준의 중대한 변경은 질의한다.

## 묶음과 자기 적용

P1=C01/C02 canonical authority/인증,실제 probe 전 readiness closed. P2=C03/C04 native 관측/적용. P3=C05/C06 launch와 안전 cleanup 동시. P4=C07/C08 generic/Cargo·jobserver. P5=C09/C10/C11 설치·bounded 후보·독립복구. P6=C12 실제 qualification/승격이다. 후속 CSRG6(P0~P5)·P1R3·DGL3·DGC3·DGA4 묶음은 별도이며 Linux 양 저장소는 연계 PR이 추가될 수 있어25는 논리 묶음 수다. launch만 있고 회수 없는 상태나 lane만 있고 buffer상한 없는 상태를 활성화하지 않는다.

기능 전까지 최소 단일Cargo job/시험thread bootstrap을 명시 기록한다. C08까지 foreground daemon이며 P4 정리 전 시험된 기능 bundle을 target 밖에 보존한다. SLO 릴리스가 아니다. C09는 manifest·보호 release/recovery·현재 사용자 LaunchAgent를 설치하며 서비스는 worktree target을 실행하지 않는다. 재시작은 기존 journal을 열어 대조하고 누락/손상은 closed다. 명시 bootstrap/repair 예외를 기록하되 자동 비관리 fallback은 없다.

C10에서 부모 기능을 먼저 시험한 뒤 그 기능이 포함된 부모를 동결하고 즉시 별도 후보를 실행한다. 후보 용량≤부모lease,격리 state/socket/자격/cache와 실제 workload의 stable-launcher 중재가 필요하다. 정상 두번째예산·제어자격은 금지한다. 이후 해당 build/test를 부모로 관리하고 실제 admission/launch/대조 receipt를 보존한다. C11은 후보 장애·실패 upgrade·drain timeout·후보 admission 없는 repair를 시험한다. C12는 부모 아래 측정한 조합만 승격한다.

## 정리와 완료

정확한 task 경로·크기·사용 여부를 조사한다. report/raw/manifest/log를 worktree 밖 보호 경로로 복사하고 hash를 검증한다. 설치 기능/복구 artifact·자격·journal·승인 문서·공유 toolchain·Cargo 다운로드를 보존한다. 완료 PR의 재생성 target/임시 docs출력만 지우고 clean task worktree·병합 입증 local branch만 일반 삭제한다. unknown/unmerged를 force로 우회하지 않는다. 원격 branch와 .local 전체는 보존한다. #65 완료 후 task docs worktree만 정리하고 원래 CodeSpace는 유지한다. du삭제크기와 전후filesystem여유실측은 별도로 보고한다.

본문은 body file로 작성하고 scope/base/head·행동/불변조건·호환성·명령/toolchain/결과·증거/CI event·의존·한계·rollback을 포함한다. secret/사용자payload/메모리citation은 공개하지 않는다. 문서rollback은 append revert/새revision과 pair갱신이며 force push로 소비중commit을 지우지 않는다. runtime은 신규 admission닫기→실제scope관측/drain→journal대조→호환artifact복귀다. off설정만으로 lease/자손 정리가 아니며 새기록 후 stale snapshot 복원 금지다. repair는 후보 admission과 독립이다.

완료는 준비+6개PR 병합/main검증,task local branch/worktree/build정리,영문/한국어 최신,보호 증거/복구 artifact,측정artifact를 쓰는 사용자서비스 검증이다. 구현과 자격을 분리하며 CodeSpace runtime·Linux·cache·추가adapter는 이번 범위 밖이며 미검증이다.
