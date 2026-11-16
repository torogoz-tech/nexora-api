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
