# Development scripts

Scripts are development and verification tools, not production server entry points.

- `scripts/validation/` checks schemas, language neutrality, lexical boundaries, and expression contracts.
- `scripts/benchmarks/` runs corpus, retrieval, incremental, and MCP measurements.
- `scripts/mcp/` verifies protocol behavior and knowledge-enrichment fixtures.
- Corpus tools validate fixture identity, expected facts, evidence, coverage, policy results, and reproducible report metadata.

Run scripts from the repository root and read their command help before using them against non-synthetic data. Benchmark output is only comparable when corpus, engine, profile, build, environment, sample count, and warmups are compatible.
