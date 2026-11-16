# Platform domains

The API is larger than the offline analyzer. It provides the authenticated boundary around engineering work and knowledge.

The main domains visible in the source include:

- installation identity, actors, roles, permissions, OIDC, and passkeys;
- projects, repositories, Git connections, webhooks, and repository workers;
- local and remote repository source bindings, provider branches, pull requests, branch protection, commits, checks, and delivery evidence;
- analysis profiles, profile selection, analysis reports, and code queries;
- documents, revisions, publications, links, context manifests, and context packages;
- knowledge contributions, reviews, synthesis, files, images, and retrieval;
- tasks, backlogs, task types, dependencies, comments, history, and approvals;
- service requests, request workspaces, attachments, links, reports, and support mutations;
- developer workflows, workflow versions, transitions, and releases;
- branch protection, code reviews, Git authors, notifications, calendars, and ratings.

These domains share scope, identity, audit, and authorization rules, but they should not be explained as if they were all part of the offline Rust engine. The web application is the human-facing client for many of these capabilities.

The three platform domains that most directly organize human work are documented in the common repository: [task management](https://github.com/torogoz-tech/nexora/blob/main/docs/task-management.md), [help desk](https://github.com/torogoz-tech/nexora/blob/main/docs/help-desk.md), and [knowledge management](https://github.com/torogoz-tech/nexora/blob/main/docs/knowledge-management.md).
