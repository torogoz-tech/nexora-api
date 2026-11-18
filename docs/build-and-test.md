# Build and test

Use the locked toolchain and workspace lockfile.

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For a local CLI run:

```bash
cargo run -p nexora-cli -- --version
cargo run -p nexora-cli -- languages list
cargo run -p nexora-cli -- analyze tests/fixtures/javascript/declarations.js
```

Some integration tests require PostgreSQL, Dragonfly, Neo4j, or provider fixtures and are marked explicitly. An ignored environment-dependent test is not evidence that an external provider or deployment works. Record the environment and command with benchmark or integration results.

## Analysis budgets

Parser, lexer-region, and repository composition limits are based on deterministic work (steps, tokens, nodes, units, and bytes), not elapsed wall time. When a repository contains files that cannot be analyzed with the configured profiles, indexing returns a partial snapshot and lists those files in `selection.unknown_files` with `analysis_status: "unknown"`. A service-level timeout may still cancel work to protect availability; it is not used to decide the contents or status of a completed snapshot.

## Authentication controls

The API applies persistent fixed-window login limits per normalized account (10 attempts per 15 minutes) and per client address (60 per 15 minutes). Configure the exact IP addresses of trusted reverse proxies with repeatable `--trusted-proxy` arguments. Forwarded addresses are honored only when the direct peer matches this allowlist. Do not trust arbitrary client-supplied forwarding headers.

Local passwords use Argon2id with explicit parameters (19 MiB, two iterations, one lane). Successful login upgrades older or weaker hashes. Sessions expire after 30 minutes without activity and have an eight-hour absolute lifetime. Passkey ceremonies accept origins from the configured corporate OIDC origin allowlist.
