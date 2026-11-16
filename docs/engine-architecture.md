# Engine architecture

The Rust workspace is divided by responsibility so that the analysis engine does not depend on the platform API or the MCP transport.

```text
profile definitions -> language validation -> lexer -> parser -> AST
                                                   -> symbols/resolution
                                                   -> graph/evidence
                                                   -> indexer -> snapshot/storage
                                                                  -> retrieval/MCP
```

Important boundaries:

- `nexora-core` owns IDs, source locations, spans, and shared errors.
- `nexora-language` loads and validates declarative profiles.
- `nexora-lexer` produces bounded token streams.
- `nexora-parser` builds CST structures from significant tokens.
- `nexora-ast` projects recognized syntax into normalized nodes.
- `nexora-symbols` records declarations and resolution states.
- `nexora-graph` creates canonical nodes, edges, and evidence.
- `nexora-indexer` coordinates file and repository analysis.
- `nexora-governance` evaluates policies against snapshots.
- `nexora-runtime` connects analysis to durable and projected stores.
- `nexora-mcp` exposes bounded query and operation contracts.
- `nexora-api` owns authenticated HTTP and platform behavior.
- `nexora-cli` is the developer-facing command boundary.

The engine uses concrete types for pipeline services. Traits are reserved for explicit storage/retrieval contracts rather than speculative abstraction layers.
