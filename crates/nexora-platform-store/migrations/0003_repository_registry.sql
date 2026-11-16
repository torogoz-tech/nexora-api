CREATE TABLE project_repositories(
 id text PRIMARY KEY CHECK(id LIKE 'repo\_%'), project_id text NOT NULL REFERENCES projects(id),
 provider text NOT NULL CHECK(provider IN ('github','gitlab')), host text NOT NULL,
 requested_full_name text NOT NULL,
 remote_id text, remote_full_name text, connection_id text,
 authorization_status text NOT NULL DEFAULT 'awaiting_authorization'
   CHECK(authorization_status IN ('awaiting_authorization','authorized','revoked')),
 archived boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
 CHECK(authorization_status <> 'authorized' OR (remote_id IS NOT NULL AND remote_full_name IS NOT NULL AND connection_id IS NOT NULL)),
 UNIQUE(provider,host,remote_id)
);
CREATE UNIQUE INDEX project_repository_requested_active ON project_repositories(project_id,provider,host,requested_full_name) WHERE NOT archived;
CREATE INDEX project_repository_page ON project_repositories(project_id,id);
