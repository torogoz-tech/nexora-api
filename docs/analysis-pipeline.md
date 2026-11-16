# Analysis pipeline

The CLI and indexer follow the same conceptual path:

```text
path
 -> validated file selection
 -> profile loading and validation
 -> lexical scan
 -> CST
 -> AST projection
 -> declarations and symbol table
 -> resolution plans
 -> graph projection
 -> diagnostics, coverage, and evidence
 -> snapshot identity
```

The repository walker rejects symlinks and parent traversal within the documented boundary, skips generated directories such as `.git`, `target`, `node_modules`, and `.nexora`, and applies cooperative size, count, depth, and time limits.

The analysis never executes source, comments, YAML instructions, or generated commands. A lexical failure is reported as a typed diagnostic; it is not converted into a successful partial pipeline. A snapshot can retain readable source context even when a file could not be analyzed successfully.

## Identity

File and snapshot identity includes the relevant source and analysis context. Changes to source, rules, diagnostics, profile configuration, or engine identity are not silently treated as the same result. CST, AST, and symbol IDs are local to a file; graph identity adds path and snapshot scope.

## Incremental reuse

The external cache stores pre-resolution file fragments. Cross-file resolution still runs for the current snapshot. A cache hit is accepted only after identity, version, content, evidence, and graph integrity checks. Cache reuse is an optimization; it is never the authoritative source of facts.
