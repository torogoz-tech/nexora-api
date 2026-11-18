# Nexora Independent Components 0.4.0 Implementation Plan

> **For agentic workers:** This plan is the implementation checklist for the approved Nexora component separation. Keep each independently deployable service in its own repository, with its own CI, image, and tests. Do not create Codex-prefixed branches.

**Goal:** Deliver Nexora 0.4.0 as independently testable and deployable repositories for the Web, API, MCP server, and four worker responsibilities.

**Architecture:** PostgreSQL remains the source of truth for jobs and domain state. The API commits a durable job before publishing its job identifier to RabbitMQ; messages are wake-up hints, not job payloads. A worker claims work from PostgreSQL with a lease and fencing token, drains eligible work after startup and after each wake-up, and acknowledges only after the durable state transition. Separate worker deployments own code analysis, Git release composition/publication, knowledge synthesis and ratings, and external notification delivery. The analyzer remains one engine pipeline; lexer, parser, graph, and indexer are not separate services. MCP becomes a standalone HTTP service behind the existing Nginx entry point and calls authenticated API capabilities.

**Tech Stack:** Rust, PostgreSQL, RabbitMQ, Docker, GitHub Actions, Vite/React, Nginx, Kubernetes manifests in the Nexora documentation repository.

**Spec:** Approved architecture discussion in this Codex thread; current implementation evidence in `crates/nexora-api`, `crates/nexora-platform-store`, `crates/nexora-mcp`, and `.github/workflows/ci.yml`.

## Global Constraints

- Release version is `0.4.0` for every existing and new Nexora repository in this delivery.
- Pull requests target `main` from `develop`; do not create `codex/*` branches.
- PostgreSQL remains authoritative; RabbitMQ does not own job state.
- Workers claim jobs with PostgreSQL locking, leases, and fencing; duplicate and stale deliveries must be harmless.
- Worker startup performs recovery of queued and expired-lease jobs before waiting for wake-up messages.
- API and workers do not share one deployed image.
- CI compiles an artifact once, saves it, and builds the container from that artifact; CI does not perform CD.
- Preserve the user-owned uncommitted change in `nexora-web/src/services/startOidcLogin.ts`.
- Do not publish, push, or change Cloudflare/production routing without explicit authorization for that external operation.

## Target Repositories

| Repository | Owns | Does not own |
|---|---|---|
| `nexora-web` (existing) | Browser client and static web image | API or MCP server runtime |
| `nexora-api` (existing) | Authenticated domain HTTP API, PostgreSQL migrations, durable job creation, RabbitMQ publication | Worker execution and MCP protocol runtime |
| `nexora-mcp` (new service repository) | Remote MCP HTTP transport, MCP auth handoff, tool schemas and API client | Direct ownership of domain persistence or the analysis engine |
| `nexora-analysis-worker` | Repository checkout, language analysis pipeline, snapshot publication | Release promotion and notification delivery |
| `nexora-release-worker` | Release/transition composition and Git branch publication | Repository analysis execution |
| `nexora-knowledge-worker` | Knowledge synthesis, RAG structuring/embedding, daily developer analysis, rating closure | External email delivery |
| `nexora-notification-worker` | Email and other external delivery attempts | Knowledge synthesis and in-app notification records |

Analysis profiles remain configuration/data, not a separate repository. Shared wire contracts are versioned and documented with the API; avoid creating a separate contracts repository unless extracting these contracts proves necessary to prevent circular dependencies.

## Review Focus

- PostgreSQL commit succeeds but RabbitMQ publish fails: job remains recoverable and is eventually claimed without relying on an outbox table.
- RabbitMQ redelivers a message or two pods wake simultaneously: exactly one current lease can commit a result.
- Worker dies after claiming: expired leases are reclaimed on worker startup and stale workers are fenced.
- Worker receives an unrelated job kind: it must not claim or strand that job.
- MCP API call fails or API is unavailable: MCP returns a protocol-valid, actionable error without bypassing API authorization.
- Existing production routes and browser same-origin `/api` behavior keep working throughout the staged cutover.

---

### Task 1: Define versioned job and service boundaries

**Files:**
- Create: `docs/architecture/components-0.4.0.md`
- Modify: `docs/overview.md`
- Test: documentation review against current job tables, lease code, and API routes

- [ ] Document job kinds, owner service, wake-up routing key, idempotency key, lease/fencing contract, startup recovery, retry policy, and failure semantics.
- [ ] Document authenticated internal MCP-to-API operations and service identity; do not reuse a browser session as service authentication.
- [ ] Record the phased compatibility/cutover plan, including current `api.nexora.infinitechsv.org` tunnel route and Nginx's existing `/api` and `/mcp` paths.
- [ ] Verify the document does not promise RabbitMQ to be the source of truth or require a new polling loop.

### Task 2: Extract RabbitMQ transport and job publication in API 0.4.0

**Files:**
- Modify: API job-creation handlers and `nexora-platform-store` job stores
- Create: RabbitMQ publisher, queue configuration, and focused delivery tests
- Modify: `.github/workflows/ci.yml`, `Dockerfile`, `CHANGELOG.md`, and API package versions

- [ ] Persist each job transactionally in PostgreSQL before publishing a durable RabbitMQ wake-up containing only job ID and kind.
- [ ] Publish with publisher confirms; on broker unavailability, return/record an explicit dispatch-pending outcome without rolling back accepted source-of-truth data.
- [ ] Add a bounded retry/reconciliation strategy that does not add a database outbox table or a permanent API polling loop.
- [ ] Make publication idempotent for a job ID and configure durable queues, persistent messages, and dead-letter handling.
- [ ] Keep CI quality, integration, one-time binary artifact, and image-from-artifact stages; rename image and artifact tags to `0.4.0`.
- [ ] Verify API compatibility and focused queue behavior before moving a worker.

### Task 3: Extract `nexora-analysis-worker` 0.4.0

**Files:**
- Create repository: `nexora-analysis-worker`
- Move the engine crates required by `nexora-indexer` as one coherent engine source tree, preserving licenses and tests.
- Create: dedicated analysis worker binary, consumer, Dockerfile, CI workflow, changelog, and focused recovery/lease tests.

- [ ] Keep lexer, parser, AST, symbols, graph, and indexer in-process as one analysis pipeline.
- [ ] Consume analysis wake-ups and claim only analysis jobs from PostgreSQL using existing lease/fencing semantics.
- [x] On startup and after each RabbitMQ reconnect, claim eligible queued and expired analysis jobs before consuming RabbitMQ messages in the current API repository.
- [ ] Remove analysis execution from the API runtime after the new worker passes equivalent integration coverage.
- [ ] Build the image from the CI artifact and tag the image `0.4.0`.

### Task 4: Extract `nexora-release-worker` 0.4.0

**Files:**
- Create repository: `nexora-release-worker`
- Move release composition, task transition resumption, and Git publication execution from `RepositoryWorker`.
- Create dedicated queue consumer, Dockerfile, CI workflow, changelog, and focused publication tests.

- [ ] Assign explicit release and transition job kinds so the analysis worker cannot claim them.
- [ ] Persist publication results under the current database lease/fencing token.
- [ ] Preserve Git network policy and credential isolation.
- [ ] Build image from artifact and tag `0.4.0`.

### Task 5: Extract `nexora-knowledge-worker` 0.4.0

**Files:**
- Create repository: `nexora-knowledge-worker`
- Move synthesis, knowledge processing/embedding, daily developer analysis, and rating closure.
- Create dedicated queue consumer, Dockerfile, CI workflow, changelog, and worker-specific tests.

- [ ] Use typed job kinds and isolated concurrency limits for model-backed tasks.
- [ ] Keep secrets/configuration server-side and avoid logging evidence or prompts.
- [ ] Recover pending work on startup and use leases to fence duplicate execution.
- [ ] Build image from artifact and tag `0.4.0`.

### Task 6: Extract `nexora-notification-worker` 0.4.0

**Files:**
- Create repository: `nexora-notification-worker`
- Move email delivery and SMTP integration from API startup.
- Create dedicated queue consumer, Dockerfile, CI workflow, changelog, and retry/deduplication tests.

- [ ] Separate durable in-app notification records from external delivery attempts.
- [ ] Claim delivery records with leases and idempotent event keys; handle SMTP retries and dead-lettering.
- [x] Remove email sending from API server startup and add a dedicated `notifications` process command in the current API repository.
- [ ] Build image from artifact and tag `0.4.0`.

### Task 7: Extract the remote MCP server 0.4.0

**Files:**
- Create repository: standalone `nexora-mcp` service
- Reuse/refactor MCP schemas and protocol code from `crates/nexora-mcp`.
- Modify API: internal authenticated tool operations and MCP key validation/introspection as required.
- Create: Dockerfile, CI workflow, changelog, protocol and authorization tests.

- [ ] Move the remote HTTP MCP endpoint out of the API process while retaining stdio support where currently used by the CLI.
- [ ] Route MCP domain operations through authenticated API capabilities; do not give MCP direct database write authority.
- [ ] Preserve existing personal MCP key scope and project permission enforcement.
- [ ] Keep Nginx `/mcp` as the public entry and route it to the MCP container only after compatibility verification.
- [ ] Build image from artifact and tag `0.4.0`.

### Task 8: Release and deployment documentation for all seven repositories

**Files:**
- Modify/create: public Nexora documentation repository, component deployment docs, Kubernetes manifests, migration and rollback procedure.
- Modify: all seven `CHANGELOG.md` / version files and GitHub Actions workflows.

- [ ] Ensure each repository has independent tests, build artifact, Docker image, health check, and `0.4.0` version metadata.
- [ ] Put Kubernetes manifests in the Nexora documentation repository, not application repos.
- [ ] Document RabbitMQ/PostgreSQL dependencies, credentials, exchanges/queues, scaling, readiness, resource sizing, observability, and rollback.
- [ ] Keep CD/operator deployment out of GitHub Actions.
- [ ] Prepare PRs from `develop` to `main` after validation; do not push or publish until explicitly authorized.

### Task 9: End-to-end verification and delivery audit

- [ ] Run focused tests for each independent repository and contract compatibility tests across API/worker/MCP boundaries.
- [ ] Verify a job survives broker outage, worker restart, duplicate wake-up, concurrent pods, lease expiry, and stale completion attempts.
- [ ] Verify all artifacts and GHCR image tags are `0.4.0` and images are built from saved artifacts, not by recompiling in Docker.
- [ ] Verify API and Web remain compatible and current user changes are preserved.
- [ ] Inspect the final repository, branch, commit, and PR state separately; report anything not pushed or externally published.
