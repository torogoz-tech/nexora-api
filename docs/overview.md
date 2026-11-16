# API and engine overview

This repository is the executable center of Nexora. It has three operating modes.

## Offline CLI

The CLI can validate profiles, analyze a file, index a directory in memory, inspect snapshots, traverse graph data, run corpus checks, and evaluate governance inputs without PostgreSQL, Neo4j, Dragonfly, or an external AI service.

## Persistent runtime

The runtime can publish immutable snapshots to PostgreSQL, project relationships to Neo4j, and reuse per-file analysis through Dragonfly. It makes publication state and recovery explicit; it does not silently fall back from persistent mode to an unrelated in-memory result.

## Authenticated platform

The API owns actors, projects, repositories, permissions, Git connections, documents, knowledge, tasks, workflows, approvals, releases, and platform MCP. Client-provided identity and paths are inputs to validation, not proof of ownership.

The web application is a separate client. The local MCP stdio server is another boundary. Public documentation must keep those three modes distinct.
