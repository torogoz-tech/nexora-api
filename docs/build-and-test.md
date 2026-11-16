# Build and Test

The supported baseline is:

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Some integration tests require local PostgreSQL or provider fixtures and are explicitly marked when an environment is required. Do not treat ignored environment-dependent tests as proof of an external provider integration.
