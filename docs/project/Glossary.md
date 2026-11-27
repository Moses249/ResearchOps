# ResearchOps glossary

| 용어 | 정의 |
| --- | --- |
| Run / 업무 실행 | 목표·범위·계약 아래 수행해야 할 작업 전체 |
| Execution session | 수동·예약·복구 등 한 번의 실행 호출과 그 관측 이력 |
| Task | claim·임대·재시도·저장이 가능한 작업 단위 |
| Delivery / 작업 전달 | 한 작업을 특정 토큰으로 작업자에게 맡긴 시도. 보고서 전달과 구분 |
| Lease / fencing token | 한시적 작업 소유권과, 이전 소유자의 늦은 쓰기를 거절하는 식별자 |
| Scope | 허용된 국가·역할·자료·단계·실행 환경 등 작업 경계 |
| Batch | 원본 관측·manifest·결과를 묶은 append-only 저장 단위 |
| Checkpoint | 저장된 실제 결과와 다음 처리 위치를 연결하는 복구 지점 |
| Readback receipt | 저장 후 별도 조회에서 hash·수량·상태를 확인한 기록 |
| System completed | 한 실행 호출이 통제된 방식으로 끝남. 업무 COMPLETE와 다름 |
| Business COMPLETE | 계약의 전체 완료 조건이 실제 저장 상태와 필요한 외부 증거로 충족됨 |
| PARTIAL | 전체 완료 조건이 미충족이며 관측된 종료 사유·남은 범위·재개 위치가 보존된 결과 |
| Manifest | 실제 관측 항목 또는 실제 렌더 결과에서 추출한 구조·수량·식별 정보 |
| Artifact | 입력 상태·템플릿 버전에 연결된 생성 결과물 |
| Outbox / sent_unconfirmed | 전달 의도를 저장한 원장 / 전송됐을 가능성이 있으나 아직 재조회로 확정하지 못한 상태 |
| Adapter | 외부 소스·모델·SQL·템플릿·전달 제공자를 공통 계약에 연결하는 경계 |
| Fixture | 재현 가능한 테스트 입력. 실제 운영 관측을 대신하지 않음 |
