# Governance engine

The governance crate compiles policy specifications into bounded evaluation plans. It evaluates attributes, relation predicates, targets, scope, coverage, evidence, and traversal limits against a snapshot.

## Result semantics

- `PASS` means the available evidence satisfies the assertion.
- `FAIL` means the assertion is contradicted or a violation is found.
- `UNKNOWN` means coverage, resolution, or available evidence is insufficient.

`UNKNOWN` is essential for safe governance. A missing relationship is not a confirmed absence, and a truncated traversal is not compliance. The evaluator records findings, evidence anchors, policy metadata, and snapshot identity so a reviewer can inspect why a result occurred.

Policy input is bounded and validated. YAML does not execute code or SQL. The current waiver manager and some remote governance transports remain contracts or future work where the limitations documentation says so.
