# Known limitations

- Language understanding is profile-driven and partial.
- The graph vocabulary is broader than the currently extracted relationship set.
- Types, complete control flow, aliases, re-exports, nested scopes, and deep call semantics are not universally available.
- `UNKNOWN` is a real result when coverage or evidence is insufficient.
- Cooperative filesystem, traversal, and evaluation limits are not hostile-process isolation or hard memory quotas.
- Local engine contracts and authenticated remote platform contracts have different authorization boundaries.
- Provider fixtures and local integrations do not certify arbitrary external accounts. GitHub has been checked in the current verification work; GitLab live verification remains scheduled for this week.
- Distributed publication is coordinated through explicit authority, checksums, locks, retries, and state transitions; it is not one distributed transaction across every store.
- Some governance waivers and future remote governance transports remain contracts where the source says so.

These limitations are intentional publication boundaries. They should be read alongside the profile's own capabilities and verification documents.
