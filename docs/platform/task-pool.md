# Task pool and explicit execution

A task can exist in the pool without a project, assignee, workflow, or execution state. Creating or reserving a task records work as `pending`; it does not mean that implementation has started. The task lifecycle (`pending`, `active`, `completed`) is separate from the configurable states inside a workflow.

## Taking work

`nexora_task_take` is the explicit boundary that starts governed work. The operation checks the expected task revision, actor authority, workflow membership, dependencies, and the published workflow version. It assigns the verified actor when appropriate, fixes the workflow version, and enters the initial state. Concurrent takes cannot both succeed against the same revision.

For implementation tasks with authorized repositories, taking work may prepare provider branches. The selected repositories and branch policy are explicit. Nexora does not silently select every repository in a project, and it does not create a branch without the required provider authorization.

## Moving through the pool

- `nexora_task_pool_list` lists visible work and supports pool-oriented filters.
- `nexora_task_take` starts a governed task for the verified actor.
- `nexora_task_transition` executes an outgoing workflow transition after guards and authority checks.
- `nexora_task_pause` and `nexora_task_resume` preserve the workflow and state while changing active participation.
- `nexora_task_move` changes backlog placement; it is not a workflow transition.
- A handoff transition can move a task to another published workflow while preserving its identity and history.

Reassignment does not silently continue execution under a new person. The task returns to the configured pending boundary when the domain requires it, and the new responsible actor must take it explicitly.

## Completion is evidence-based

A terminal workflow state may complete one stage without proving that a release reached production. Nexora distinguishes local work, commit, provider publication, pull-request review, merge, checks, deployment, and production delivery. A task is globally complete only when the configured final workflow and its required evidence establish that outcome.

All mutations use current revisions and idempotency keys where required. Historical task, transition, approval, branch, evidence, and release records remain available for audit and for the next agent or developer.
