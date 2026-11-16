# Nexora API

Nexora API is the Rust server and code-intelligence engine for the Nexora ecosystem. It includes the CLI, deterministic analysis pipeline, language package loading, graph and snapshot models, persistence adapters, governance evaluation, MCP services, and platform authorization.

## Requirements

- Rust stable, as pinned by `rust-toolchain.toml`.
- PostgreSQL, Dragonfly/Redis-compatible cache, and Neo4j only for the persistent platform runtime.
- No database or external AI service is required for offline CLI analysis.

## Development

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the CLI with a profile directory:

```bash
cargo run -p nexora-cli -- --version
cargo run -p nexora-cli -- languages list
cargo run -p nexora-cli -- analyze tests/fixtures/javascript/declarations.js
```

## Repository layout

- `crates/` contains independently focused Rust packages.
- `schemas/` contains public data contracts.
- `languages/` contains the current built-in profile snapshot for development compatibility.
- `scripts/` contains validation, benchmark, and MCP tooling.
- `infra/` contains local and deployment support.
- `policies/`, `corpus/`, and `examples/` contain reproducible demonstrations and test inputs.

## Scope and limitations

Nexora does not execute analyzed code. Semantic coverage depends on the selected profile and is intentionally explicit. Unsupported syntax, incomplete relationships, unresolved references, and `UNKNOWN` governance results remain visible. See `docs/limitations.md` and the profile repositories for details.

## License

MIT. See `LICENSE`.
