# ResearchOps glossary

English translation of the [Korean glossary](Glossary.md).

| Term | Definition |
| --- | --- |
| Run / business workflow | All work required under an objective, scope, and contract |
| Execution session | One manual, scheduled, or recovery invocation and its observed history |
| Task | A unit of work that can be claimed, leased, retried, and persisted |
| Task delivery | An attempt that assigns a task to a worker using a specific token; distinct from report delivery |
| Lease / fencing token | Temporary task ownership and an identifier used to reject late writes from an earlier owner |
| Scope | Authorized country, role, sources, stages, execution environment, and other work boundaries |
| Batch | An append-only persistence unit containing source observations, manifest, and results |
| Checkpoint | A recovery point linking actual persisted results to the next processing position |
| Readback receipt | A record that a separate post-write query verified hashes, counts, and states |
| System completed | One invocation ended in a controlled way; distinct from business COMPLETE |
| Business COMPLETE | The contract's full completion conditions are satisfied by actual stored state and required external evidence |
| PARTIAL | Full completion conditions remain unmet, with an observed stop reason, remaining scope, and resume position preserved |
| Manifest | Structure, counts, and identity information extracted from actual observations or rendered output |
| Artifact | Generated output bound to an input state and template version |
| Outbox / sent_unconfirmed | The authoritative record of delivery intent / a state where transmission may have occurred but readback has not confirmed it |
| Adapter | A boundary connecting external sources, models, SQL, templates, or delivery providers to shared contracts |
| Fixture | Reproducible test input; not a substitute for a production observation |
