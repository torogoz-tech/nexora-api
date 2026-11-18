# API and engine overview

This repository is the executable center of Nexora. It has three operating modes.

## Offline CLI

The CLI can validate profiles, analyze a file, index a directory in memory, inspect snapshots, traverse graph data, run corpus checks, and evaluate governance inputs without PostgreSQL, Neo4j, Dragonfly, or an external AI service.

## Persistent runtime

The runtime can publish immutable snapshots to PostgreSQL, project relationships to Neo4j, and reuse per-file analysis through Dragonfly. It makes publication state and recovery explicit; it does not silently fall back from persistent mode to an unrelated in-memory result.

## Authenticated platform

The API owns actors, projects, repositories, permissions, Git connections, documents, knowledge, tasks, workflows, approvals, releases, and platform MCP. Client-provided identity and paths are inputs to validation, not proof of ownership.

The web application is a separate client. The local MCP stdio server is another boundary. Public documentation must keep those three modes distinct.

## 0.4.0 component separation (target)

The 0.4.0 architecture separates the Web, authenticated API, remote MCP server, repository analysis worker, Git release worker, knowledge/rating worker, and external notification worker into seven independently built and deployed repositories. The lexer, parser, graph, and indexer remain one in-process analysis engine; analysis profiles remain configuration, not a service. PostgreSQL continues to own job and domain state, while RabbitMQ carries durable wake-up messages that direct workers to claim authoritative work from PostgreSQL. See [the component boundary and migration design](architecture/components-0.4.0.md).

This is the target architecture, not a claim that the extraction or production cutover is already complete. The current API workspace still embeds worker code and has cross-crate dependencies; the staged plan tracks those removals.
