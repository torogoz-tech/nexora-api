# Profile contract

The language engine consumes four YAML documents per profile: `language.yaml`, `lexer.yaml`, `grammar.yaml`, and `graph.yaml`.

The JSON schemas under `schemas/` define syntax. Domain validators additionally check references, captures, progress, recursion, resource budgets, and compatible schema versions. This two-level validation is important: valid YAML is not automatically a valid analysis profile.

Profiles are data, not Rust branches. The engine interprets their patterns, transitions, productions, captures, projections, and resolution routes. A new profile can therefore be maintained independently of the common engine, subject to the engine's published capability versions.

## Evidence required from a profile

Every profile should provide fixtures for supported syntax, partial or rejected syntax, diagnostics, graph output, and known limitations. Its repository should explain what “supported” means and should not use the existence of a schema field as proof of semantic coverage.
