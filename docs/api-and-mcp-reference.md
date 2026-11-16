# API and MCP reference

Nexora has two related but different integration surfaces:

1. the authenticated HTTP platform API;
2. the local or remote MCP catalogs.

They share domain concepts but do not automatically expose the same operations. Always inspect the current server catalog and authorization before calling an operation.

## HTTP platform API

The HTTP API owns authenticated installation, project, repository, Git connection, task, document, knowledge, workflow, approval, release, and provider operations. Routes are registered in `crates/nexora-api/src/api_server.rs`; handlers and store policies are the source of behavior.

HTTP mutations generally require an authenticated session or authorized API key, project/repository/resource scope, a current revision where the resource is mutable, an idempotency key for retryable creation or provider work, workflow and approval requirements, and provider confirmation for remote operations.

## MCP catalogs

The local code MCP provides bounded snapshot queries such as repository overview, symbol lookup, references, dependencies, graph traversal, code search, source context, and knowledge context. It is fixed to its configured root and tenant.

The platform MCP exposes authorized operations for projects, repositories, synchronization, tasks, documents, knowledge, workflows, requests, releases, and evidence. A tool name does not grant permission. The caller must discover current schemas and check permissions.

## Example: repository context

```text
nexora_repository_overview(repository_id, snapshot_id)
nexora_find_symbol(name: "PaymentService")
nexora_get_dependencies(node: "...")
```

Results are scoped to the returned tenant, repository, and snapshot. Empty search results do not prove absence, and truncated traversal does not prove that no relationship exists.

## Example: governed task work

```text
nexora_task_get(task_id)
nexora_task_relations_get(task_id)
nexora_task_work_prepare(...)
nexora_task_work_report(...)
nexora_task_transition(...)
```

Preparation and reporting preserve provider branch facts; they do not independently prove push, merge, or deployment. The actual workflow and provider evidence determine the next valid transition.

## Errors and uncertainty

Clients must distinguish authorization denial, invalid revision, provider rejection, rate limiting, temporary unavailability, conflict, and unknown provider outcome. On an unknown outcome, reconcile the provider before retrying. Reuse the same request identity only with the same payload.

## Public reference status

The source schemas and MCP catalogs are authoritative for `0.1.0`. A generated OpenAPI portal, language SDKs, and a single combined API reference remain release work rather than claims of the current repository.
