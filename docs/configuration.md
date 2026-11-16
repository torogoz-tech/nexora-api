# Configuration

Keep credentials and installation state outside the repository. Use private files with restrictive permissions for operator secrets and provider credentials. Do not commit `.env` files, access tokens, private keys, database passwords, or production URLs containing credentials.

Offline analysis can run without external services. Persistent indexing requires the services described by `infra/local`. Remote provider integrations require explicit installation configuration and authorization.
