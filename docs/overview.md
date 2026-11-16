# API Overview

The API repository is the executable part of Nexora. It has three operating modes:

1. Offline CLI analysis for local, deterministic inspection.
2. Persistent indexing and retrieval for a self-hosted installation.
3. Authenticated API and MCP access for authorized users, agents, and integrations.

The server is the authority for authorization, tenant/repository scope, snapshot publication, and governance decisions. Client-provided paths, actor identities, and provider secrets are never trusted as ownership claims.
