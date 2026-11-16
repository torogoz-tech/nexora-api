# Configuration and operations

Keep credentials and installation state outside the repository. Use private connection files with restrictive permissions for operator secrets and provider credentials. Never commit `.env` files, access tokens, private keys, database passwords, or credential-bearing production URLs.

Offline analysis requires only the engine and a profile directory. Persistent indexing requires PostgreSQL, a Dragonfly/Redis-compatible cache, and Neo4j as described in the runtime documentation. Remote provider integrations require explicit installation configuration and authorization.

The local runtime is a development and self-hosting boundary, not a claim that every deployment has production-grade isolation by default. Configure TLS, identity, network policy, backups, secret management, observability, and process limits for the environment in which it is operated.
