# MCP boundary

Nexora's local MCP server runs as a JSON-RPC process over stdin/stdout. Diagnostics go to stderr. It is not an HTTP server and it does not automatically edit a user's client configuration.

The process is fixed to an authorized root and tenant. Tools cannot use that process to index an arbitrary second root. Read tools can expose repository overviews, symbols, references, dependencies, graph traversal, code search, source context, snapshots, and knowledge context according to the active configuration.

Indexing and knowledge-writing capabilities are explicit opt-ins. A client that can query context does not automatically gain permission to edit source, approve a workflow, or publish a policy decision.

The remote platform MCP catalog is a different boundary from the local stdio analyzer. It uses installation identity, keys, roles, permissions, and project scope. Documentation must not present the two catalogs as the same service.
