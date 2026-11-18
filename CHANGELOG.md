# Changelog

## 0.4.0

- Move snapshot, graph, query, cache, and knowledge runtime persistence behind the versioned `nexora-runtime` package in `nexora-job-store`.
- Allow database connections to use a validated private DNS name or IP only when `allow_remote_host` is explicitly enabled; existing configurations remain loopback-only by default.
- Added the versioned RabbitMQ job wake-up contract and durable publisher topology while keeping PostgreSQL as the job source of truth.
- Added PostgreSQL recovery scans for repository, release, knowledge, daily-analysis, transition, and email work, with coalesced mutation signals and broker reconnect recovery.
- Moved task transition operations onto the shared `nexora-task-workflow` package; API requests now persist durable AI review/rating work and return an accepted response while the knowledge and release workers complete the transition.
- Changed the repository worker to recover pending analysis at startup, consume RabbitMQ wake-ups, drain eligible PostgreSQL jobs, and acknowledge only after processing.
- Removed email delivery from API server startup and added a dedicated notification-worker command with PostgreSQL recovery on startup and broker reconnect.
- Updated API artifact and OpenAPI release metadata to `0.4.0`.

## 0.3.0

- Added global ascending and descending alphabetical ordering to service-request listings before pagination.
- Bound source analysis and cross-language composition by deterministic work budgets; repository analysis now records unsupported or skipped files as `UNKNOWN` while returning a partial snapshot.
- Precompiled lexer rule sets to reduce per-token regular-expression checks, and added graph integrity integration coverage.
- Added persistent account and peer throttling for password login, explicit Argon2id parameters with opportunistic rehashing, and equalized password checks for SSO-only and unknown accounts.
- Bound browser sessions to a 30-minute idle timeout and an eight-hour absolute lifetime.
- Validate passkey request origins against the configured OIDC origin allowlist; added repeatable `--trusted-proxy` configuration for client address resolution.
- Added a branded OIDC callback error page for invalid or expired authorization links.

## 0.2.1

- Align the API build runner and runtime on Ubuntu 24.04 to remove the GLIBC compatibility warning.
- Include Git and its HTTPS transport for automatic repository synchronization.
- Smoke-test the assembled container on pushes and pull requests before publishing the versioned image from main.

## 0.2.0

- Prepared the API runtime for the versioned container release.
- Allowed task commits to be linked when they exist in the repository, regardless of whether they are present in the task branch.
- Removed the edit reason and immutable purpose fields from shared task-type changes while preserving revision checks and audit history.
- Updated the public task-type contract and OpenAPI documentation.

## 0.1.0

- Initial open-source publication of the Nexora Rust API and analysis engine.
