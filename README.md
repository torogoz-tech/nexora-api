# Nexora API and analysis engine

This repository contains the executable Rust side of Nexora. It is both the deterministic code-intelligence engine and the server-side boundary that turns analysis into a self-hosted platform.

## Architecture and stewardship

Nexora's architecture and initial implementation are led by [Abner Velasco](https://www.linkedin.com/in/abnervelasco/), who is also responsible for the ongoing maintenance and evolution of this repository.

## What lives here

- The profile-driven lexer, parser, AST, symbol, resolution, and graph pipeline.
- Snapshot creation, identity, evidence, coverage, and diagnostics.
- Offline and persistent CLI commands.
- Incremental file reuse and the external cache path.
- PostgreSQL authority, Dragonfly-compatible reuse cache, and Neo4j graph projection.
- Deterministic governance policies and findings.
- Local MCP stdio services and platform MCP catalogs.
- Authenticated platform APIs for projects, repositories, knowledge, documents, tasks, workflows, approvals, Git integrations, and releases.

The repository does not contain the web interface or independently maintained language profiles. See the common Nexora documentation and the profile repositories for those boundaries.

## Three ways to run it

1. **Offline analysis:** inspect files and profiles without databases or external AI services.
2. **Persistent indexing:** publish versioned snapshots and query them through the local runtime.
3. **Platform service:** expose authenticated projects, repositories, knowledge, work, workflows, and MCP operations.

## First commands

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo run -p nexora-cli -- languages list
cargo run -p nexora-cli -- analyze tests/fixtures/javascript/declarations.js
```

## Read the implementation guide

- [Engine architecture](docs/engine-architecture.md)
- [Analysis pipeline](docs/analysis-pipeline.md)
- [Profiles and schemas](docs/profile-contract.md)
- [Runtime storage](docs/runtime-storage.md)
- [MCP](docs/mcp.md)
- [Governance](docs/governance.md)
- [Platform domains](docs/platform-domains.md)
- [API and MCP reference](docs/api-and-mcp-reference.md)
- [Build and test](docs/build-and-test.md)
- [Configuration and operations](docs/configuration.md)
- [Limitations](docs/limitations.md)

## Release boundary

This repository is version `0.1.0`, under the ecosystem policy documented in the common Nexora repository. The engine's support is profile- and fixture-specific. A graph enum, schema capability, or API route is not by itself evidence of complete support for every possible source construct. The repository documents those boundaries deliberately.

## License

MIT. See [LICENSE](LICENSE).
