> **phase:** implement · Design + tasks approved 2026-11-28 · owner @Moses249

# Feature: execution-completion-contract

[English version](execution-completion-contract.en.md) · [Project/Plan](../project/Plan.md) · [Project/Design](../project/Design.md)

## Plan

### Summary

연구 실행이 작은 묶음·중간 저장에서 멈추지 않고, 실제 계약 조건에 따라 계속되거나 구체적인 부분 완료 사유와 복구 위치를 남기게 한다. 수동·향후 예약 어댑터의 호출은 같은 실행 Runner를 사용하며, 시스템 호출 종료와 업무 완료를 구분한다.

### Problem and baseline

기존 MVP에는 SQL 작업 큐, 만료 임대 복구, 토큰 기반 쓰기 방어, 원문 인용 검사와 모델 검증 루프가 있다. 그러나 호출별 작업 수 제한과 세션 메모리의 진행 상태만으로는 저장 후 재조회·현재 결과의 업무 완료·호출 중단 이후의 복구 근거를 일관되게 보장하지 못한다.

첨부 운영 계약의 공통 지속성 요구를 제품에 반영한다. 기존 실행 계약 초안은 참고 구현이며, 승인된 AC와 설계에 맞춰 채택·수정·제외한다. 기존 PR #1의 검증은 이 기능의 검증으로 전용하지 않는다.

### Goals

- 버전이 고정된 계약, 명시적인 실행 세션, 영구 체크포인트를 제공한다.
- 미완료 우선 처리와 작은 묶음의 연속 실행을 제공한다.
- SQL 저장·재조회·현재 입력 연결을 완료 조건으로 만든다.
- 부분 결과에서 실제 실패 위치와 안전한 재개 방법을 확인할 수 있게 한다.

### Non-goals

- 실제 채용 사이트 탐색·순위 계산·고정 HTML 렌더러·Gmail 발송.
- 기존 Supabase 운영 프로젝트·권한·스키마·다른 자동화의 변경.
- OS/클라우드 스케줄러 설치, 다중 호스트 실행, 플랫폼 강제 종료 방지.
- 원문 운영 계약을 자동 해석하여 임의 SQL·코드로 실행하는 기능.

### User scenarios

1. 연구 실행을 시작하면 이전 중단 지점과 저장 확인 누락을 먼저 복구하고, 다음 처리 가능한 작업을 계속 실행한다.
2. claim이 비어 있어도 지연 재시도·다른 유효 임대·실패·근거 부족을 구분하여 남은 상태를 표시한다.
3. DB 오류나 명시적 실행 한도에 도달하면 실제 수량·실패 단계·복구 SQL과 안전한 복구 JSON을 확인한다.
4. 전체 작업이 처리돼도 역할별 증거가 없으면 업무 COMPLETE로 표시되지 않는다.

### Acceptance criteria

| ID | 검증 가능한 조건 |
| --- | --- |
| **AC-1** | 실행 시작 전 저장된 계약 버전·body·기대 digest를 실제 SELECT로 검사한다. digest 불일치·미지원 정책·잘못된 한도는 작업 claim·모델 호출·전달 전에 거절하며, 외부 텍스트를 실행 지시로 해석하지 않는다. 기본 로컬 계약과 명시적 새 버전 등록을 구분한다. |
| **AC-2** | 새 Runner는 작은 묶음의 크기를 전체 실행 종료 조건으로 쓰지 않는다. 3개 처리 가능한 작업을 batch size 1로 시작하면 계약의 다른 차단이 없는 한 한 세션에서 3개 모두 처리하고 결과를 저장·재조회한다. 매 작업 또는 묶음 성공 직후 다음 처리 가능 작업/단계로 진행한다. |
| **AC-3** | 선택된 run·실행 mode 범위에서 미완료 readback과 재시도/만료 작업을 새 작업보다 먼저 복구한다. 다른 연구 run을 임의 실행하지 않는다. SQL claim 우선순위와 stale-token·lease 만료 쓰기 거절을 동시 작업자·복구 테스트로 검증한다. |
| **AC-4** | 소스·관측/체크포인트·receipt·Artifact 원장은 append-only다. 결과·체크포인트·해당 큐 전이는 같은 트랜잭션으로 저장하며, 커밋 후 별도 SELECT의 hash·수량·상태 확인 receipt가 있어야 저장 검증으로 인정한다. 원본/체크포인트 직접 UPDATE·DELETE와 근거 변조는 거절되거나 무결성 실패로 표시된다. |
| **AC-5** | claim 0건을 업무 완료로 해석하지 않는다. SQL에서 pending·유효 running lease·지연 재시도·failed·근거 누락을 구분하고 다음 가능 시각을 표시한다. 아직 처리 가능한 다른 항목이 있으면 먼저 진행하며, 예산 내 재시도 가능 시점이 오면 같은 세션에서 계속한다. |
| **AC-6** | API·CLI·화면에서 시스템 세션 상태와 업무 상태를 별도로 제공한다. 업무 COMPLETE는 필수 작업·근거·저장 readback·현재 입력에 대응하는 보고서 Artifact가 모두 충족될 때만 허용한다. 모델 검증을 요구한 계약은 실제 승인 결과가 필요하며, worker가 종료됐다는 사실만으로 완료를 표시하지 않는다. 기존 `status` 응답은 호환성을 유지하되 업무 완료 판단은 새 필드로 명시한다. |
| **AC-7** | SQL 재시도는 busy/locked 등 분류된 일시 오류에만 최초 시도 후 최대 2회 적용한다. 비멱등 쓰기·이미 커밋된 결과·전달을 맹목적으로 replay하지 않는다. 커밋 후 receipt 생성 전에 중단되면 실제 저장 상태를 재조회해 복구하고 결과를 중복 생성하지 않는다. |
| **AC-8** | PARTIAL 실행 기록에는 시작·종료 KST 기준일, 실제 전후 수량, 마지막 성공 단계, 실패 단계·비밀을 제거한 실제 오류, 구체적 stop code, run ID·SQL·바인딩 값·다음 가능 시각이 포함된다. DB 저장 불가 시 같은 근거를 로컬 복구 JSON으로 보존하고 저장 성공을 주장하지 않는다. |
| **AC-9** | 계약은 구체적인 실행 예산을 가진다. 기본 제안은 최대 900초·batch size 20·SQL 재시도 2회이며 최대 예산은 3600초다. 한도 종료는 설정값·실측 경과·미완료 수를 기록한다. 단순한 묶음 수·막연한 시간 부족은 stop code가 될 수 없다. lease 만료는 중단 원인 미확인으로 기록하며 장애 원인을 추정하지 않는다. |
| **AC-10** | manual·scheduled·recovery trigger가 같은 Runner와 완료 Gate를 사용함을 테스트한다. 이 기능은 실제 scheduler를 설치하지 않는다. 재시작 후 미완료 세션과 scope 내 복구 후보가 DB에서 관측되고, 필요한 API 자격 증명이 없으면 재연결 필요 상태를 표시한다. |
| **AC-11** | 기존 MVP DB·연구 결과를 보존하는 migration과 구버전 데이터 조회를 검증한다. 기존 결과에 없던 readback·모델 승인·전달 증거를 있었다고 만들어 넣지 않는다. 이전 Artifact나 현재 입력과 맞지 않는 보고서는 새 업무의 완료 Gate를 충족하지 못한다. |
| **AC-12** | 외부 SQL·순위·템플릿·실제 렌더 manifest·Gmail 증거를 요구하는 정책은 어댑터/증거가 없으면 해당 Gate가 차단된다. Fixture를 production 완료로 인정하지 않는다. 키·인증 헤더·비공개 수신자 설정은 공개 응답/로그/복구 JSON/Git에 노출하지 않으며, 화면은 키보드와 텍스트로 진행·Gate·오류·재개 위치를 확인할 수 있다. |

### Dependencies and decisions

- 확정된 Project/Plan·Project/Design과 기존 Rust/SQLite/API 기준선을 사용한다.
- 구현 브랜치는 기준선 PR #1이 main에 병합된 뒤 main에서 생성한다. 이 Plan은 PR #1 병합을 수행하거나 승인하지 않는다.
- SQLite에서 먼저 검증하며 PostgreSQL·실제 Supabase·Gmail 연동은 후속 기능이다.
- 현재 공급 문서 연구의 run+mode가 이 기능의 scope다. 국가·역할별 외부 큐는 해당 어댑터 기능에서 확장한다.
- 원문 계약을 그대로 실행하지 않는다. 지원하는 구조화된 정책을 버전·digest로 고정한다. 외부 기존 계약/템플릿 digest 검증은 실제 연결 전까지 차단한다.
- 새 공통 Runner와 legacy bounded 작업 API의 연결·호환 방식은 기능 Design에서 명시하며, UI의 실제 연구 실행은 새 Runner를 사용한다.
- milestone/릴리스 버전은 지정하지 않는다.
- 기능 범위에 영향을 주는 미해결 질문은 없다. 기본 한도는 이 Plan 체크포인트의 승인 대상이다.

### Verification approach

가짜 clock과 오류 주입을 사용하여 예산·lease·재시도를 재현하고, 임시 DB에서 실제 트랜잭션·SELECT·append-only 제약을 검증한다. 모델 호출은 모의 provider로 재현한다. UI/API/CLI의 상태·복구 자료를 대조하며, fresh read-only QA가 모든 AC를 독립적으로 확인한다. 실제 production 작업 완료는 이 테스트의 주장 범위가 아니다.

## Design

### Impact — implementation read contract

| 파일 | 변경 |
| --- | --- |
| `src/contracts.rs` (신규) | 지원 정책 타입·한도 검증·버전/digest registry·run binding |
| `src/checkpoints.rs` (신규) | 원자 checkpoint·실제 SQL readback·입력 fingerprint·Artifact·멱등 복구 |
| `src/execution.rs` (초안 교체) | 공통 Runner·실행 세션·clock·budget·대기/재개·완료 평가·복구 JSON |
| `src/engine.rs` | 버전별 migration·run 생성/계약 binding 원자화·scoped claim·결과 저장 경계·legacy 호환 |
| `src/agents.rs` | 기존 루프에서 단일 claim 처리 분리; 모델 검증/revision 루프와 단계 이력 유지 |
| `src/provider.rs` | 남은 예산을 반영한 timeout, 호출 전후 heartbeat 연결, 안전한 오류 redaction |
| `src/web.rs`, `src/main.rs` | 공통 실행·계약·복구 조회 API/CLI; legacy bounded 경로 유지 |
| `src/lib.rs`, `Cargo.toml`, `Cargo.lock` | 모듈 등록; chrono·MD5 호환성 의존성; 잘못 남은 `report_contract` 선언 제거 |
| `migrations/003_execution.sql` (초안 교체) | 계약·scope·세션·checkpoint·receipt·Artifact·readback 원장과 제약 |
| `web/app.js`, `web/index.html` | 새 실행 경로·계약 선택·업무/시스템 상태·Gate·재개 정보 표시 |
| `tests/execution_contract.rs`, `tests/migration.rs` (신규) | 계약·연속 실행·오류 주입·복구·구버전 호환 검증 |
| `tests/reliability.rs`, `tests/agents.rs`, `tests/http.rs` | stale write·모델 예산·백그라운드 실행·새 응답/legacy 회귀 |
| `README.md`, `docs/architecture.md` | 실제 새 동작·legacy 의미·복구·한도를 반영 |

### Contract and scope

- 지원 JSON 정책만 실행한다. 버전, profile (`research`, `model_reviewed_research`, `weekly_report`), runtime, batch size, SQL retry count를 고정한다. 기본값은 승인된 900초·20건·2회, 허용 범위는 runtime 1..3600초·batch 1..100·SQL retry 0..2다.
- Registry는 append-only다. SHA-256 무결성과 필요한 MD5 호환 digest를 확인하며, digest는 신뢰 주체 인증이 아니다. 계약 등록과 별도 SELECT 검증 후에만 미시작 run에 binding할 수 있다.
- run 생성, 소스/질문 저장, 기본 계약 binding을 한 트랜잭션으로 처리한다. 시작 때 기본 계약을 몰래 다시 binding하거나 기존 digest 불일치를 덮어쓰지 않는다.
- 실행 scope는 run ID와 최초 실행 mode다. 작업 시작 후 mode나 계약 변경은 거절한다. 모델 검증을 요구하는 계약은 local mode로 우회할 수 없다.
- `weekly_report`의 외부 proof 요구는 명시적 미지원 Gate로 반환한다. 이 기능에서 해당 데이터를 만들어 넣거나 발송 함수를 노출하지 않는다.

### Shared Runner and clock

`execute(run_id, mode, trigger, clock)`는 모든 trigger의 진입점이다. `Clock` 경계는 UTC 시각·단조 경과 시간·대기를 제공하며, 테스트에서는 가짜 clock으로 시간 경과를 제어한다. KST 날짜는 UTC에서 명시적으로 계산한다.

1. 계약·mode·연결·scope를 검사한다. 거절된 시작은 진단 정보만 남기며 작업을 claim하지 않는다.
2. 만료 세션을 관측·기록하고, live session이 있으면 중복 Runner를 거절한다. before counts와 session lease를 저장한다.
3. 이미 커밋된 결과의 누락 receipt를 우선 복구한다. 그다음 재시도/만료 작업을 새 작업보다 우선 claim한다.
4. local executor 또는 Researcher/Verifier executor로 claim 하나를 처리한다. 원문/답변/모델 승인은 결과 원자 저장 경계에 연결한다.
5. 커밋 → 별도 SQL readback → receipt 저장 → 재조회된 after counts/마지막 성공 단계 기록을 수행한다. 다음 묶음을 반복한다.
6. claim이 없으면 상태를 다시 읽는다. 예산 안에 도래하는 retry/lease는 heartbeat를 유지하며 기다리고, 다른 eligible 작업은 먼저 처리한다.
7. 작업 범위가 종료되면 현재 입력 fingerprint에 맞는 Markdown Artifact를 저장·재조회한다. 필수 Gate를 실제 SQL로 평가해 session과 업무 상태를 별도로 기록한다.
8. 구체적 blocker·한도·실패가 발생하면 저장 여부를 확인하고 PARTIAL 기록 또는 안전한 복구 JSON을 남긴다.

budget은 단조 시간으로 측정하고 lease는 저장된 UTC로 판정한다. 초기 session/task lease는 300초, heartbeat 목표 간격은 30초이며 모델 호출 전후에도 갱신한다. 모델 요청 timeout은 `min(기존 30초, 남은 예산)`으로 줄인다. 이미 만료된 token은 heartbeat·결과·종료 쓰기 모두 거절한다. 알려진 다음 처리 시점이 예산을 넘으면 `next_eligible_after_budget`와 실제 timestamp를 기록한다.

### Data, transactions, and idempotency

| 데이터 | 구현 기준 |
| --- | --- |
| `execution_contracts`, `run_contracts`, 실행 scope | append-only 계약 원문; binding은 첫 claim 전에만 변경 가능; run/mode fingerprint 고정 |
| `execution_sessions` | session ID, run/mode/trigger, UTC 시작·종료, KST 시작·종료일, lease, 단조 경과 값, 상태, stage, 전후 수량, 오류·resume |
| `execution_batches` | task ID·delivery token·결과/원본 참조·안정된 payload·hash·count; result/queue와 같은 commit |
| `batch_readbacks` | 실제 SELECT로 계산한 hash/count/state와 확인 시각; 단순 '성공' flag로 대체 금지 |
| `report_artifacts`, `artifact_readbacks` | body hash와 입력 fingerprint에 모두 연결; unique key는 run+input fingerprint+body hash |

입력 fingerprint에는 계약 참조·실행 scope·실제 소스 hash·질문/작업 결과·모델 검증 증거를 안정된 정렬로 포함한다. 과거 Artifact가 같은 본문을 가졌다는 이유만으로 최신 입력을 충족하지 않게 한다.

SQLite `user_version`을 트랜잭션 안에서 읽고 필요한 migration만 순서대로 적용한다. 기존 M1/M2 결과를 삭제하거나 과거 receipt를 가공하지 않는다. 예전 결과는 legacy proof 부족을 표시하며, 확인 가능한 현재 readback을 수행할 때도 과거에 검증됐다고 기록하지 않는다.

DB 재시도 helper에는 재실행이 안전한 operation만 전달한다. Busy/locked는 제한 backoff로 최대 2회 재시도한다. operation ID·멱등 key는 재시도 밖에서 생성한다. 커밋 뒤 응답/readback 실패는 같은 작업을 새로 실행하지 않고 SQL로 기존 commit을 찾아 receipt만 복구한다. 검증 실패·오래된 token·제약 위반에는 자동 SQL replay를 하지 않는다.

### Completion, diagnostics, and recovery

- `system_status`: running/completed/failed/interrupted. `business_status`: IN_PROGRESS/PARTIAL/COMPLETE.
- Gate 목록: pinned contract, scope/mode, required tasks, evidence, storage integrity/readbacks, current Artifact, 계약상 모델 검증, 외부 필수 proof. 해당 없는 Gate와 미연결 Gate를 구분한다.
- claim 0건·batch 경계·LLM approval은 완료 근거가 아니다. legacy `status`는 유지하되 UI의 업무 판정은 새 `execution` 정보를 사용한다.
- stop code는 `required_work_complete`, `evidence_gaps`, `retry_limit_reached`, `execution_budget_exhausted`, `next_eligible_after_budget`, `provider_unavailable`, `contract_mismatch`, `storage_failure`, `integrity_failure`, `session_lease_expired` 등 관측 사실을 설명한다.
- PARTIAL은 모든 AC-8 필드를 구조화하여 저장한다. resume SQL과 값은 분리하고 사용자 텍스트를 SQL 문자열에 끼워 넣지 않는다.
- 실패 정보는 허용 필드만 출력하며 API 키·Authorization·비공개 설정을 제거한다. 복구 JSON은 DB 옆 `recovery/`의 새 파일로 원자 저장하고 기본 Git ignore 범위에 포함한다. 파일 저장도 실패하면 저장 불가 사실과 민감 정보를 뺀 진단을 호출자에게 반환한다.

### Interfaces and compatibility

| 경로 | 동작 |
| --- | --- |
| `POST /api/contracts` | 지원 정책의 version/body/digest를 등록·재조회 |
| `POST /api/runs/{id}/contract` | 실행 전 계약 binding 검증 |
| `POST /api/runs/{id}/execute` | mode·trigger를 받아 background Runner 시작; 수락과 완료를 구분 |
| `GET /api/runs/{id}` | 기존 응답 + `execution`의 system/business 상태·counts·Gate·resume |
| `GET /api/recovery` | 선택 조건에 맞는 미완료 session/run의 진단 목록; 다른 run을 자동 실행하지 않음 |
| `GET /api/runs/{id}/recovery` | 안전한 복구 정보/JSON; 키·수신자 설정 제외 |
| CLI `execute`, `contracts import`, `contract bind`, `recovery` | HTTP와 동일한 검증·Runner·출력 모델 사용 |

기존 `/work`, `/agents`, CLI `work`의 bounded 동작은 호환용 low-level 경로로 유지한다. 계약/scope 검사는 적용하되 이 경로 종료만으로 business COMPLETE를 부여하지 않는다. UI의 Create & run / Resume / Retry는 새 `/execute`를 사용한다. scheduled trigger는 공통 Runner 호출 계약만 제공하며 cron이나 자동화는 설치하지 않는다.

### Existing draft reconciliation

| 초안 요소 | 결정 |
| --- | --- |
| 계약 타입·hash·KST 함수 | 지원 정책과 한도를 명확히 하여 재사용 |
| append-only table/trigger | 전체 ledger/readback 누락을 보완하여 채택 |
| `create()` 후 별도 기본 binding | 원자성 문제를 고쳐 같은 생성 transaction에 통합 |
| `begin_execution()`의 자동 기본 binding | 불일치·누락 은폐를 막도록 제거; legacy는 별도 진단 |
| receipt count만으로 완료 평가 | 실제 payload/현재 입력/검증 결과를 검사하도록 교체 |
| body hash만의 Artifact unique key | 입력 fingerprint를 포함하도록 수정 |
| 미연결 `report_contract` module 선언 | 제거; 이 기능의 범위는 외부 Gate 차단까지 |
| 기존 worker의 limit 종료 루프 | shared Runner를 새로 구현하고 legacy wrapper와 구분 |

### AC → design mapping and verification strategy

| AC | 설계 항목 | 주요 검증 |
| --- | --- | --- |
| AC-1 | Contract registry/binding, preflight | digest·정책·mode 불일치 시 claim/model 호출 0회 |
| AC-2 | Shared Runner, batch 반복 | batch 1·task 3에서 한 session의 처리·readback 3개 |
| AC-3 | scope 고정, 회복 우선 claim, fencing | 복구 우선순위·다른 run 불변·동시/stale write |
| AC-4 | 원자 batch와 append-only/readback | rollback·변조·UPDATE/DELETE 거절·hash/count/state 일치 |
| AC-5 | 대기 분류와 fake clock | 빈 claim에 COMPLETE 금지·다른 eligible 처리·지연 작업 재개 |
| AC-6 | Completion evaluator, 현재 Artifact | system completed/business PARTIAL·모델 검증 Gate·입력 변경 |
| AC-7 | idempotent SQL helper, commit 조회 복구 | busy/locked 2회 한도·비일시 오류 미재시도·commit 후 crash |
| AC-8 | durable diagnostics·recovery JSON | 전후 실측·stage·SQL/params·DB/파일 동시 실패·redaction |
| AC-9 | monotonic budget·UTC lease | 경계값·실측 elapsed·허용 stop code·원인 미확인 |
| AC-10 | trigger 공통 경로, DB recovery 조회 | 세 trigger 결과 대조·재시작·자격 증명 부재 |
| AC-11 | versioned migrations, legacy 상태 | v1/v2 DB 보존·가짜 receipt 금지·오래된 Artifact 무효 |
| AC-12 | unsupported proof Gate, 안전한 UI/API | 외부 호출 0회·fixture 제한·비밀 비노출·키보드/텍스트 확인 |

실제 SQLite와 오류 주입, 가짜 clock, 모의 provider로 검증한다. baseline 회귀 테스트·fmt·Clippy를 실행한 뒤 fresh read-only QA가 AC별 증거를 검토한다. 스크린샷만으로 DB Gate나 실제 전달을 검증했다고 표시하지 않는다.

### Proposed task decomposition

아래는 승인 대상 작업 분해이며 live 완료 상태는 이슈 #2에서 관리한다. Design 승인 뒤 체크리스트로 반영한다.

| Task | 작업 | AC |
| --- | --- | --- |
| T1 | 계약 registry·정책 검증·원자 run binding·scope 고정 | AC-1, AC-3, AC-12 |
| T2 | 버전별 migration·ledger/receipt 제약·legacy 진단 | AC-4, AC-11 |
| T3 | 원자 결과/checkpoint·별도 readback·현재 입력/Artifact fingerprint | AC-4, AC-6, AC-7, AC-11 |
| T4 | session·clock·heartbeat·지속 loop·대기/예산/trigger | AC-2, AC-3, AC-5, AC-9, AC-10 |
| T5 | local/model executor 통합·timeout·멱등 SQL retry | AC-1, AC-3, AC-7, AC-9 |
| T6 | 업무 Gate·부분 결과·안전한 DB/파일 복구 정보 | AC-6, AC-8, AC-10, AC-12 |
| T7 | API/CLI와 legacy 호환 경로 연결 | AC-1, AC-6, AC-8, AC-10, AC-11 |
| T8 | UI 계약·system/business 상태·Gate·resume·접근성 | AC-6, AC-8, AC-12 |
| T9 | 오류/시간/재시작/동시성/호환 테스트·사용 문서 | AC-1~AC-12 |
| T10 | 독립 QA 준비·Draft PR 증거 정리 (실제 독립 검증은 test 단계) | AC-1~AC-12 |

### Risks and alternatives

- 기존 bounded endpoint의 의미를 바꾸는 대신 새 실행 경로를 선택해 호환성 위험을 줄인다. UI가 legacy를 계속 호출하는 회귀는 통합 테스트로 막는다.
- SQLite 단일 호스트 범위에서 lease/session 중복 실행을 SQL로 막는다. 메모리 job 목록만으로 소유권을 판단하지 않는다.
- DB retry는 전체 callback 자동 재호출 대신 허용 operation으로 제한한다. commit 이후 부작용 중복이 핵심 위험이다.
- 모델 요청 중 예산·lease가 소진될 수 있으므로 호출 경계 timeout과 heartbeat를 실제 clock 테스트로 검증한다.
- 과거 원본의 제약 추가와 migration 실패는 실제 v1/v2 fixture·백업 복사본으로 검증한다. 운영 DB를 직접 시험하지 않는다.
- 새 라이브러리는 초안의 chrono(UTC/KST)와 md5(레거시 호환) 범위로 제한한다. SHA-256은 기존 sha2를 사용한다. 범용 workflow framework·새 큐·HTML parser는 도입하지 않는다.

## Verification

승인된 설계에 따라 구현한 후 독립 QA의 AC별 관측 결과를 기록한다. 기존 테스트 결과나 구현 초안을 PASS로 전용하지 않는다.
