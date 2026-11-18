# Nexora component boundaries for 0.4.0

## Purpose

Nexora 0.4.0 separates independently deployable workloads so each repository can own its image, CI surface, focused tests, and scaling policy. PostgreSQL remains the source of truth. RabbitMQ is the notification mechanism that wakes workers; it is not the durable job ledger.

## Deployment units

| Unit | Responsibility | Primary scaling signal |
|---|---|---|
| Web | Static browser application | HTTP traffic / static cache |
| API | Authentication, authorization, domain commands/queries, migrations, job creation | HTTP concurrency and DB pool |
| MCP | Remote MCP protocol, tool catalog, API client | MCP sessions / request concurrency |
| Analysis worker | Checkout and code analysis through the complete language pipeline | Pending analysis jobs, CPU and memory |
| Release worker | Workflow transitions, release composition, Git branch publication | Pending release jobs and Git latency |
| Knowledge worker | Knowledge synthesis/structuring, embeddings, daily analysis, ratings | Pending model jobs and provider limits |
| Notification worker | SMTP and other external delivery attempts | Pending delivery jobs and provider limits |

The existing lexer, parser, AST, symbol, graph, and indexer modules stay together as an in-process engine used by the analysis worker. Analysis profiles are data/configuration and do not become a service. No additional service is created for rating closure; it stays with knowledge/rating workloads. In-app notifications remain durable domain records owned by the API/database; this worker handles external delivery.

### Dependency boundary that must be removed

The current crate graph is not a service graph: `nexora-api` directly uses `nexora-indexer`, `nexora-runtime`, and `nexora-reasoning`; `nexora-platform-store` also depends on indexer, runtime, reasoning, and retrieval; and the `nexora-mcp` library depends on indexer/runtime/graph/storage. Merely copying worker entry points into separate repositories would preserve these compile and ownership couplings. Before extraction is complete:

- Move worker-owned execution and persistence modules to the owning worker repositories, with versioned package dependencies only for narrowly scoped reusable domain types.
- Split the current broad platform store by ownership so the API does not pull analyzer or worker-only reasoning code simply to serve API traffic.
- Make the remote MCP process use the API client and remove its direct engine/runtime/store dependencies. Keep stdio MCP as a separate client mode if it still needs local engine queries.
- Keep the analyzer's internal crates together in the analysis worker repository; they do not become separately deployed services.

The CI goal is both fewer tests per repository and less unnecessary compilation per pipeline. A package split that only relocates tests while preserving the API's full transitive engine dependency does not satisfy the performance objective.

## Job lifecycle and broker semantics

The first wire contract is deliberately small and versioned:

```json
{"version":1,"job_id":"job_123","kind":"repository_index"}
```

`kind` is one of `repository_sync`, `repository_index`, `task_transition_resume`, `release_composition`, `knowledge_synthesis`, `knowledge_processing`, `daily_developer_analysis`, or `email_notification`. Routing keys are `nexora.jobs.analysis`, `nexora.jobs.release`, `nexora.jobs.knowledge`, and `nexora.jobs.notification`; each worker binds only its own queue. Consumers reject unknown envelope versions, unknown fields, malformed IDs, and unsupported kinds. Payloads are reloaded from PostgreSQL after claiming.

1. The API validates the command and commits the durable job row in PostgreSQL. The job row records the kind, tenant/project/repository scope, source revision, current state, attempts, and timestamps.
2. After the database commit, the API publishes a persistent RabbitMQ wake-up containing only the job identifier and kind. Publisher confirms distinguish broker acceptance from a failed publish. No user payload, credential, prompt, or repository contents go in the wake-up.
3. A worker receives a wake-up and treats it as a hint to inspect PostgreSQL. It claims eligible work through a transaction using row locks with `SKIP LOCKED`, a lease owner, an expiring lease, and a monotonically changing fence/attempt.
4. At process startup and after every RabbitMQ reconnect, each worker scans/claims its own eligible queued jobs and expired leases. It begins or resumes waiting for RabbitMQ deliveries only after that recovery pass.
5. A worker may drain multiple eligible jobs after one wake-up. It acknowledges the RabbitMQ delivery after it has completed the drain/claim attempt; job completion itself is persisted in PostgreSQL under the current fencing token.
6. Duplicate, delayed, and out-of-order wake-ups are safe: PostgreSQL state and lease fencing decide ownership. A stale worker cannot publish completion after lease replacement.
7. On transient execution failure, the worker records a bounded retry schedule in PostgreSQL. Permanent failures reach a terminal state and remain inspectable. Rabbit dead-lettering handles malformed/unroutable notifications, not business job state.

### Current test concentration (static source inventory)

The current workspace has about 453 `#[test]` attributes in `nexora-indexer`, 305 in `nexora-platform-store`, and 105 in `nexora-api`; the API crate also has 90 integration-test files totaling roughly 14,820 lines, and the platform store has 150 integration-test files totaling roughly 32,385 lines. These source counts are a sizing signal, not proof that each CI test passes. The analyzer and worker-owned persistence tests must move with their service boundaries; API HTTP/auth/domain tests remain in API CI.

### Broker outage and missed notification

The design deliberately has no transactional outbox table. The API commits the job first and signals a background publisher. The API currently returns the persisted job response without a separate RabbitMQ delivery-status field; the queued PostgreSQL record is the durable acceptance result. The publisher waits with exponential connection backoff while RabbitMQ is unavailable, and after connecting it scans PostgreSQL for eligible jobs and republishes one wake-up per job kind. A successful enqueue also signals an immediate scan. This is reconnect/event-triggered recovery rather than a steady database polling loop. Each worker recovers eligible jobs at startup and after RabbitMQ reconnects, so jobs remain actionable when the API or broker was down. The analysis worker currently implements both recovery points in the API repository; the other worker processes remain to be extracted and verified. This behavior must be exercised against RabbitMQ and PostgreSQL before claiming the split complete.

## Persistence and trust boundaries

- `connections.json` accepts loopback PostgreSQL hosts by default. Kubernetes or other non-local deployments must set `allow_remote_host: true` explicitly and provide a syntactically validated DNS hostname or IP. Operators must route that name to an approved private database endpoint and restrict access with network policy; the current PostgreSQL connector does not configure TLS.
- PostgreSQL owns job state, domain records, authorization scope, idempotency, leases, and final results.
- RabbitMQ transports wake-up IDs only. Queue durability, message persistence, publisher confirms, consumer acknowledgements, bounded prefetch, and dead-letter routing are required.
- Each worker receives only the database permissions and secrets it needs. It does not trust message fields for authorization or scope; it reloads the job and related ownership from PostgreSQL.
- Repository analysis and release workers keep Git credentials and outbound network policy isolated from API/MCP.
- MCP authenticates its client key and delegates domain access to authenticated API capabilities. It does not connect directly to PostgreSQL or duplicate domain authorization.
- External delivery workers read only eligible notification records and SMTP/provider configuration. In-app notification history remains database-backed.
- Nginx remains the host's single public application entry point. Production cutover changes `/mcp` to the MCP service and leaves `/api` on API. Cloudflare Tunnel routing changes are a separate operator action after origin routing and controls have been verified.

## Repository ownership

The target is seven versioned repositories: `nexora-web`, `nexora-api`, `nexora-mcp`, `nexora-analysis-worker`, `nexora-release-worker`, `nexora-knowledge-worker`, and `nexora-notification-worker`. Each has its own `develop` and `main`, tests, GitHub Actions CI, artifact, Dockerfile, health/readiness behavior, changelog, and `0.4.0` version. CI builds the executable once, stores it as an artifact, and constructs the image from that artifact. CI does not deploy.

Engine crates are distributed to the analysis worker as source or versioned internal packages; service-to-service Cargo path dependencies may not remain after extraction. Before choosing a package distribution mechanism, measure its CI compile graph and confirm the organization's publishing/credential policy. The API should no longer compile analyzer internals merely to serve HTTP.

## Staged migration

1. Lock API/worker/MCP wire contracts and explicit job kinds without changing public production routes.
2. Add RabbitMQ publication and consumer infrastructure while preserving PostgreSQL claims and current API compatibility.
3. Extract analysis, release, knowledge/rating, and external notification workers one responsibility at a time. Keep the old consumer disabled only after equivalent queue/lease behavior passes.
4. Extract the MCP HTTP service and route MCP protocol through authenticated API operations; preserve local stdio MCP clients.
5. Update Web/API/MCP/worker images, all versions/changelogs, and per-repository CI. Keep CI artifact-to-image and no-CD boundaries.
6. Add deployment and Kubernetes manifests to the public Nexora documentation repository. Document migrations, secrets, scaling, probes, observability, rollback, and queue recovery.
7. Validate API/Web/MCP compatibility and job recovery under broker outage, duplicate wake-up, concurrency, expired lease, and abrupt process termination before production cutover.

## Operational gate

Do not remove the existing API or MCP tunnel route until the Nginx route has been verified against the same externally observable behavior, TLS/forwarded-header handling, authentication, request size limits, timeouts, rate limits, and health checks. Keep the current Cloudflare Tunnel and Nginx configuration unchanged while building the independent services.
