# 설계 개정 2: 선언된 adapter를 통한 외부 구현

설계 기준일: 2026-09-30. 대상: DevGuard의 의존성·upstream 소스 정책, 그리고 DevGuard와 CodeSpace 문서에서 DevGuard를 Codex
의존성이 없는 것으로 기술한 문장. 저장소 문서 정책에 따라 [영문](../design-revision-2.md)이 정본이며, 이 문서는 검토된 한국어
대응 문서입니다.

**상태.**
- 2026년 9월 28일 소유자는 DevGuard가 명시적인 adapter 경계 뒤에서 Codex와 그 밖의 외부 의존성을 사용할 수 있다고
  결정했습니다. 검토된 불변 소스 정체성을 쓰고, 정당한 경우 pin을 유연하게 선택합니다.
- 2026년 9월 30일 소유자는 이 결정을 설계 개정으로 기록하도록 지시했습니다.
- [설계 참조](design.md), `AGENTS.md`, [결정 기록](planning/decisions.md), [계획 문서](planning/README.md)가 이를 반영합니다.
- 역사적 승인본 [design.ko.md](../design.ko.md)와 그 [checksum](../design-source.json)은 변경하지 않으며,
  [설계 개정 1](design-revision-1.md)은 작성 당시의 문구를 유지합니다.
- 이 개정은 이후 변경을 구속하는 정책을 바꿉니다. 구현·의존성·pin·gate·계약·마일스톤·qualification 상태는 바꾸지
  않습니다.

## 1. 결정

1. **선언된 경계로만.** DevGuard는 Codex를 포함한 외부 구현을 선언된 adapter 또는 binding 경계를 통해서만 소비하며, 각 경계는
   검토된 불변 pin을 가집니다(3절).
2. **authority에 제품 타입 없음.** authority core(`devguard-contract`, `devguard-core`)와 CodeSpace가 링크할 범용 client 계약에는
   CodeSpace·Codex 제품 타입, 모델 세션, CodeSpace workspace 권한, PTY 소유권을 넣지 않습니다.
3. **정당화와 비용 통제.** 각 의존성은 무엇을 대체하는지, 그리고 그 비용을 어떻게 통제하는지로 정당화합니다(4절).
4. **gate는 첫 구성 요소와 함께 바뀝니다.** `scripts/validate.py`의 실행 가능한 의존성 gate는 첫 실제 adapter 구성 요소를 추가하고
   그 경계를 시험하는 검토된 PR에서만 바뀝니다. 이것이 gate 변경 G입니다(5절). 그때까지 gate는 모든 `codex-`·`codespace-`
   package를 거절합니다.
5. **아무것도 선택하지 않습니다.** 이 개정은 의존성·pin·후보 구조·구현을 선택하지 않습니다. CSRG-C00·CSRG-C03·CSRG-C09를
   되살리지 않으며, CS-RG 구현 중지는 계속 유효합니다.

이 개정은 다음을 앞으로를 향해 대체합니다.
- 설계 개정 1의 D3 선택 C0, 곧 “현재 구현에는 Codex 의존성을 추가하지 않는다”
  ([개정 1, 10.3절](design-revision-1.md#103-d3--devguard-의존성-정책))와 그 재검토 트리거;
- DevGuard의 기본 배포와 공용 client가 현재의 공학적 선택으로서 Codex 의존성 없이 유지된다는 모든 문장.

D3의 경계는 유지합니다. 결정 2의 계층에는 계속 제품 타입을 넣지 않습니다. 개정 1의 D1·D2와 실행 소유권 절은 바꾸지 않으며,
구현 지시로서 계속 효력이 중지된 상태입니다.

이 개정을 작성할 때 DevGuard에는 Codex 의존성이 없었습니다. `main` `4898259`의 `Cargo.lock`에는 package 80개가 있으며
`codex-*`나 `codespace-*` 이름은 하나도 없습니다. 이는 현재 graph에 대한 사실이지 규칙이 아닙니다.

## 2. 경계와 배치

아래 이름은 책임을 뜻하며 crate 이름이나 기존 API가 아닙니다.

| 경계 | 소유 | 얻어서는 안 되는 것 |
| --- | --- | --- |
| authority core(`devguard-contract`, `devguard-core`, native backend, daemon) | admission, 회계, attempt 상태, 압력, 증거 규칙 | CodeSpace·Codex 제품 타입; 모델·세션·login·agent 의미 |
| 범용 client 계약(`devguard-client`: transport, framing, 자격 전달) | 제한된 인증 메시지와 오류 변환 | CodeSpace가 이 계약을 링크하므로 Codex 타입; 암묵적인 비관리 fallback; 두 번째 authority |
| 실행 준비 | 일회성 준비, helper 호출, attachment 소유, 중단과 정리 | PTY 할당, 호출자의 spawn 인수 |
| upstream 실행 binding(선언된 adapter; 아직 없음) | DevGuard의 필요를 범용 upstream 계약에 대응시키는 일 | binding 밖의 Codex 타입; authority core나 범용 client 계약에서의 도달 가능성 |
| CodeSpace 자원 adapter(계획; CS-RG 중지) | opt-in 설정, attempt 연결, capability·결과 변환 | DevGuard만을 위한 PTY 할당·native spawn·회수자; `off`의 DevGuard 의존 |
| CodeSpace의 기존 실행 backend | 기존 OS 메커니즘(승인되면 범용적으로 확장) | DevGuard lease·요청·자격에 대한 지식 |

- **Codex binding은 공용 client graph 밖에 둡니다.** Codex가 필요한 binding은 CodeSpace가 링크하는 범용 client graph 밖에
  둡니다. 예외는 그 실행 파일의 Codex 소스 정체성을 의도적으로 통일하고 함께 qualification한 경우뿐입니다(3.3절).
- **공용·기본 빌드를 배제하지 않습니다.** 이 개정은 공용 빌드나 기본 빌드의 의존성을 금지하지 않습니다. 다만 그런 의존성은
  무엇을 대체하고 비용을 어떻게 통제하는지 보여야 합니다.
- **optional 선언은 격리가 아닙니다.** Cargo feature 통합이 다른 경로로 optional 의존성을 켤 수 있습니다.

## 3. upstream 소스와 pin

### 3.1 목표

DevGuard는 외부 구현을 adapter 뒤에서 선택·qualification·갱신·되돌립니다.
- upstream이 내부를 리팩터링했다는 이유만으로 도메인 계약을 바꾸지 않습니다.
- upstream 동작이 실질적으로 바뀌면 adapter는 그 변경을 거절하거나, 명시적으로 변환하거나, 검토된 계약 변경을 요청합니다.
  호환성이 여전히 유지되는 척하지 않습니다.
- 유연성은 시험을 거친 어느 revision을 선택하느냐에 적용되며, 움직이는 참조로 빌드하는 데에는 적용되지 않습니다.

### 3.2 후보 분류

| 분류 | 사용 | 승격 조건 |
| --- | --- | --- |
| 전체 commit으로 해석한 release tag | 충분하면 우선 | 실제 commit, artifact, 해석된 의존성 graph, 동작, target 조합 확인 |
| 아직 release되지 않은 명시적 commit(prerelease 포함) | 유효; 금지하지 않음 | release를 기다리는 것이 부족한 이유; snapshot qualification; rollback 증거 보존 |
| upstream에 먼저 제안하는 작은 downstream patch | 필요할 때 임시로 | 원래 commit, patch digest, upstream 제안과 그 상태, 분기와 제거 시험 |
| registry package | 유효 | registry, 잠근 version과 checksum, feature, target, 출처, 갱신 정책 |
| 움직이는 branch 또는 변경 가능한 PR head | 승격하지 않음 | qualification 전에 불변 commit으로 해석 |

“upstream에 먼저”는 upstream이 받아들이는 경로를 뜻합니다. Codex는 외부 코드 기여와 pull request를 받지 않으므로, Codex에
대한 제안은 분석을 담은 issue입니다. 제출에는 소유자의 별도 승인이 필요합니다.

### 3.3 실행 파일 정체성

- **같은 실행 파일.** DevGuard 구성 요소가 CodeSpace 실행 파일에 링크되면, 그 실행 파일에는 검토된 Codex 소스 정체성이
  하나만 있습니다. 그것은 CodeSpace의 gitlink입니다.
  - CodeSpace가 한 Codex 정체성을 가져오고 링크된 DevGuard crate가 다른 정체성을 가져오는 graph는 허용하지 않습니다. 같은
    crate의 두 소스는 서로 다른 crate로 컴파일되며 각자의 프로세스 전역 상태를 가집니다.
  - CodeSpace가 링크하는 범용 client 계약에는 이후 명시적인 설계 결정이 그 계약을 바꾸지 않는 한 Codex·CodeSpace 제품 타입을
    넣지 않습니다.
  - SHA가 같다는 사실만으로 동작 호환성을 입증하지 않습니다.
- **별도 실행 파일.** DevGuard는 자신의 실행 파일(daemon, helper, command line) 안에서만, 또는 CodeSpace 실행 파일에 링크되지
  않는 adapter 안에서만 Codex를 소비할 수 있습니다. 그렇다면 두 pin을 함께 옮길 필요가 없습니다.
  - qualification 대상은 client·wire·helper·capability·artifact의 조합입니다.
  - Codex Rust 타입이나 소스 정체성은 이 제품 경계를 넘지 않습니다.
- **조용한 이동 없음.** DevGuard만의 upstream 갱신은 CodeSpace의 gitlink를 옮기지 않습니다. CodeSpace만의 갱신은 설치된 DevGuard
  release를 바꾸지 않습니다.
- **아직 강제하지 않음.** CodeSpace의 gate는 아직 `codex-` crate의 두 번째 소스를 거절하지 않습니다. DevGuard crate를 CodeSpace
  graph에 처음 넣는 PR이 실행 파일 단일 정체성 검사를 추가합니다.

version 불일치를 해결한다는 이유로 PTY·spawn·회수·수명주기 소유권을 CodeSpace에서 DevGuard로 옮기지 않습니다.

### 3.4 pin 기록

소비하는 소스마다 권한 있는 검토 가능한 기록이 하나씩 있습니다. 구성 요소가 생기면 gate가 이를 검증합니다.

| 항목 묶음 | 내용 |
| --- | --- |
| 정체성 | 저장소 또는 registry, package 이름, 전체 commit 또는 잠근 package 정체성; tag는 주석일 뿐 |
| 선택 | 기준과 후보, 이유, 필요한 capability, 존재 여부와 계약 적합성의 구분 |
| 소비 | adapter 소유자, 제품 root, target, feature, normal/build/dev 분류 |
| 재현성 | lock digest, 소스 tree 또는 patch digest, 컴파일러·도구 version, 획득 방법 |
| 호환성 | adapter 계약 version, wire/helper/journal/config 영향, 알려진 비호환 조합 |
| 출처 | 라이선스와 `NOTICE` 처리, 저작자 표시, 공급망 검토, 확인한 보안 권고 |
| qualification | 보고서, 실행/건너뜀/미실행 사례, artifact hash, 기준 비교 |
| rollback | 마지막으로 승인된 조합, schema 제약, drain·reconcile 필요 사항 |
| 분기 | local patch, upstream 추적 참조, 아직 필요한 이유, 제거 또는 재수렴 조건 |

선택하지 않은 값은 `TBD - not selected`로 적고, 기계 검증되는 운영 기록에는 넣지 않습니다. SHA나 checksum을 지어내지
않습니다.

### 3.5 qualification과 승격

1. 기준을 기록합니다.
2. 후보를 선택하고 그 분류를 기록합니다.
3. target마다 관련 upstream 변경과 해석된 graph를 읽습니다.
4. 검토 가능한 branch에서 adapter를 갱신합니다.
5. adapter의 적합성 시험을 실행하고, 이어서 영향받는 제품 회귀 시험을 실행합니다.
6. 필요한 플랫폼을 qualification하고 증거를 공개합니다.
7. 정확한 head에 대한 merge 승인을 요청하고, merge 후 결과를 확인합니다.
8. artifact는 배포 gate를 통해서만 승격합니다.

증거 규칙:
- 컴파일 성공은 동작 호환성이 아니며, tag가 같다는 것은 artifact hash가 같다는 뜻이 아닙니다.
- 소스나 lock 입력이 바뀌면, 계속 적용된다는 것을 보이지 않는 한 후보 보고서는 무효입니다.
- 각 gate는 `passed`, `failed`, `skipped-by-plan`, `not_run` 중 하나를 이유와 함께 기록합니다. 플랫폼 runner가 없는 것은 통과가
  아니며, 소스 검토는 runtime 결과가 아닙니다. 실패한 시도는 보존합니다.
- 더 새로운 upstream revision으로 DevGuard 자신의 실행 파일을 qualification해도 CodeSpace의 pin이 qualification되는 것은
  아닙니다.

### 3.6 분기와 출처

- downstream patch는 임시입니다. 원래 commit, patch digest, 범위, 이유, 의도한 동작 차이, 시험, 재검토 조건, 제거 또는 재수렴
  조건을 기록하고, 받아들여지는 경로로 upstream에 제안합니다.
- 가져와 고친 upstream 코드도 같은 출처를 기록합니다.
- 의존성 검사를 피하려고 crate를 복사하는 숨은 vendoring은 계속 금지입니다.
- 의존성을 추가하는 PR은 그 라이선스 저작자 표시와 `NOTICE` 처리를 함께 추가합니다.

### 3.7 되돌리기

되돌리기는 다음을 함께 복원합니다.
- 소스 선택, lock, patch;
- 저작자 표시;
- capability와 동작 문서.

그다음 영향받는 gate를 다시 실행합니다. 점유 중인 작업을 가진 runtime은 기존의 admission 닫기·drain·reconcile 절차를 따릅니다.
이전 release가 읽지 못하는 journal schema에는 명시적인 migration 또는 drain 절차가 필요하며, commit을 되돌리는 것만으로는
runtime rollback이 아닙니다.

## 4. 의존성의 정당화

의존성 PR은 그 의존성이 DevGuard에서 무엇을 대체하는지 보입니다. 예를 들어 `HelperCommand`, launcher, native 관측의 일부입니다.
그리고 비용을 어떻게 통제하는지 보입니다.
- target마다 해석된 normal·build·dev graph와 그 feature;
- 최소·실제 컴파일러 version;
- binary 크기와 빌드 시간;
- 추가되는 runtime 작업·thread·buffer.

근거로 쓰는 package 수에는 SHA, target, feature, graph 구분, 실행 명령이 함께 있어야 합니다.

PR은 target별 metadata와 별도 빌드 호출로 다음 부정 조건을 입증합니다.
- `codex-core`, `codex-exec`, `codex-app-server`, `codex-login`은 어떤 DevGuard root에서도 도달할 수 없습니다.
- `codex-` package는 선언된 binding root에서만 도달할 수 있습니다.
- contract crate의 의존성은 바뀌지 않습니다.
- CodeSpace의 governance 없는 빌드와 runtime은 DevGuard 서비스나 자격 없이, 의도하지 않은 컴파일러 최저 version 변경 없이
  동작합니다.

`default-features = false`만으로는 증거가 되지 않습니다.

## 5. 실행 가능한 gate

현재 `scripts/validate.py`의 dependency-boundary 단계는 다음을 합니다.
- DevGuard graph에서 이름이 `codex-`나 `codespace-`로 시작하는 모든 package를 거절합니다.
- workspace root를 명시적 map과 같게 유지하고, workspace 내부 edge를 정확히 검사합니다.
- contract crate의 의존성을 `serde`, `serde_json`, `sha2`로 제한합니다.

이 개정은 그 단계를 바꾸지 않습니다.

gate 변경 G는 첫 실제 adapter 구성 요소를 추가하는 검토된 PR에서만, 그 경계에 대한 시험과 함께 이루어집니다. G는 이름
접두어 거절을 다음으로 바꿉니다.
1. 모든 root에 대한 금지 집합;
2. root별 허용 upstream crate. 선언된 binding root만 승인된 upstream crate에 도달합니다;
3. target별·비 dev graph에서 다른 어떤 root도 `codex-` package에 도달하지 않는다는 검사.

workspace map, edge 검사, contract 규칙은 유지합니다. G는 이 단계를 끄지 않으며, 그 구성 요소에 필요한 범위를 넘어 좁히지도
않습니다.

## 6. 이 개정이 바꾸지 않는 것

- 의존성·pin·후보·라이브러리·구조를 선택하지 않으며, 어떤 Codex revision도 승인하지 않습니다.
- 구현·계약·wire·journal·gate·CI·시험·서비스·자격·qualification 상태는 바뀌지 않습니다.
- CS-RG는 계속 중지 상태이며, CSRG-C00·CSRG-C03·CSRG-C09를 되살리지 않습니다. 개정 1의 D1·D2·실행 소유권 절은 그대로입니다.
- 역사적 승인본, 개정 1의 작성 당시 문구, 날짜가 붙은 인계 기록은 고쳐 쓰지 않습니다.
- `NOTICE`는 사실인 동안 그대로 두며, `docs/contracts.md`는 구현된 동작이 바뀔 때만 바뀝니다.
- CodeSpace 자체의 pin과 검토 절차는 바뀌지 않습니다.

## 7. 이 개정이 갱신한 위치

| 위치 | 처리 |
| --- | --- |
| `AGENTS.md` | 개정 목록과 의존성 항목이 이 정책을 기술합니다. gate의 현재 동작을 적습니다 |
| `README.md` 계획 문단 | Codex 무의존 구절을 이 개정을 가리키는 문장으로 바꿉니다. 저장소 전체 검색에서 발견했으며, 앞선 목록에는 없었습니다 |
| `docs/design.md`와 한국어 대응 문서 | 개정 링크와 ‘책임과 정체성’의 의존성 근거; 보류 표기가 이를 밝힙니다 |
| `docs/milestones.md` | Codex 무의존 문장을 이 개정을 가리키는 문장으로 바꿉니다 |
| `docs/planning/README.md`와 한국어 대응 문서 | 기준일, 읽는 순서, 개정 요약; 보류 표기가 이를 밝힙니다 |
| `docs/planning/decisions.md`와 한국어 대응 문서 | 이 개정의 기준 행을 추가합니다. ADR-006의 D3 행과 의존 경계 문단은 2026-09-27 문구를 유지하고 날짜가 붙은 주석을 답니다; 표기가 이를 밝힙니다 |
| `docs/planning/codespace-integration.md`와 한국어 대응 문서 | D3 항목은 문구를 유지하고 날짜가 붙은 주석을 답니다; 표기가 이를 밝힙니다 |
| CodeSpace `docs/codex-reuse.md`, `docs/upstream-update.md`와 한국어 대응 문서 | CodeSpace 대응 PR이 갱신합니다 |

의도적으로 유지하는 것:
- `docs/design.ko.md`;
- `docs/design-revision-1.md`와 그 한국어 문서;
- CS-RG를 다시 계획할 때까지 `docs/planning/milestones/CS-RG.md`와 그 한국어 문서;
- `NOTICE`와 `docs/contracts.md`;
- `docs/handoff/`의 날짜가 붙은 기록;
- `scripts/validate.py`;
- 계속 사실인 문장. 예를 들어 `README.md`의 `devguard-core` 설명, 그리고 이후의 Codex pin 변경은 검증에 근거한 별도
  결정이라는 문장.

## 8. 출처

- **결정.** 소유자의 2026-09-28 결정은 [upstream adapter 작업 명세](../handoff/2026-09-28-cs-dg-upstream-adapter-work-spec-1.md)
  3.1절과 추적 이슈에 기록되어 있습니다. 소유자는 2026-09-30에 이를 이 개정으로 기록하도록 지시했습니다.
- **pin 정책.** 3절은 [W0–W2 packet](../handoff/2026-09-28-upstream-adapter-packet.md) 4–6절의 초안을 고쳐 채택합니다. 그 packet과
  [W3 결정 packet](../handoff/2026-09-28-w3-decision-packet.md)은 날짜가 붙은 비규범 기록이며 출처로만 인용합니다. 규범 원본은
  이 개정입니다.
- **upstream 기여 정책.** Codex의 `docs/contributing.md`, 정책 commit `31f23b6`(2026-08-17).
