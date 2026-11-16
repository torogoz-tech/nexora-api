# Runtime storage and publication

The persistent runtime separates canonical storage from projections and reuse.

## Publication lifecycle

The runtime stores a pending snapshot, projects graph and query data, verifies checksums and counts, and only then marks the snapshot ready/current in PostgreSQL. A projection failure must not advance the current pointer. Repeating an identical publication is idempotent; changing content under the same key is rejected.

This is not a distributed transaction across PostgreSQL, Dragonfly, and Neo4j. The design instead makes authority, publication state, checksums, locks, retries, and recovery explicit.

## Store roles

- PostgreSQL: durable authority and platform metadata.
- Dragonfly/Redis-compatible cache: reusable per-file analysis fragments with TTL.
- Neo4j: graph traversal projection.
- pgvector-backed structures: reviewed knowledge and code-context retrieval.

Queries first establish that a snapshot is published in the authoritative store. Cache failure affects reuse, not correctness. Direct administrative queries against a projection are not equivalent to a runtime query against a published snapshot.
