CREATE TABLE feature_catalogs (
 project_id text PRIMARY KEY REFERENCES projects(id), revision bigint NOT NULL DEFAULT 0 CHECK(revision>=0)
);
CREATE TABLE project_features (
 id text PRIMARY KEY, project_id text NOT NULL REFERENCES projects(id), name text NOT NULL,
 description text NOT NULL DEFAULT '', revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 archived boolean NOT NULL DEFAULT false, created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(id,project_id)
);
CREATE UNIQUE INDEX project_features_name ON project_features(project_id,lower(btrim(name)));
CREATE TABLE feature_proposals (
 id text PRIMARY KEY, project_id text NOT NULL REFERENCES projects(id), requirement_id text NOT NULL,
 requirement_version_id text NOT NULL, requirement_title text NOT NULL, catalog_revision bigint NOT NULL,
 reason text NOT NULL, status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
 links jsonb NOT NULL, tasks jsonb NOT NULL, result jsonb, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 proposed_by text NOT NULL REFERENCES actors(id), decided_by text REFERENCES actors(id), decision_reason text,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), decided_at timestamptz,
 UNIQUE(id,project_id), FOREIGN KEY(requirement_version_id,requirement_id,project_id) REFERENCES project_document_versions(id,document_id,project_id),
 CHECK((status='pending' AND decided_by IS NULL AND decided_at IS NULL) OR (status<>'pending' AND decided_by IS NOT NULL AND decided_at IS NOT NULL))
);
CREATE TABLE requirement_features (
 project_id text NOT NULL, requirement_id text NOT NULL, feature_id text NOT NULL,
 impact text NOT NULL, proposal_id text NOT NULL,
 PRIMARY KEY(project_id,requirement_id,feature_id),
 FOREIGN KEY(requirement_id,project_id) REFERENCES project_documents(id,project_id),
 FOREIGN KEY(feature_id,project_id) REFERENCES project_features(id,project_id),
 FOREIGN KEY(proposal_id,project_id) REFERENCES feature_proposals(id,project_id)
);
CREATE TABLE task_feature_links (
 project_id text NOT NULL, task_id text PRIMARY KEY, feature_id text NOT NULL, requirement_id text NOT NULL,
 FOREIGN KEY(task_id,project_id) REFERENCES tasks(id,project_id),
 FOREIGN KEY(project_id,requirement_id,feature_id) REFERENCES requirement_features(project_id,requirement_id,feature_id)
);
CREATE INDEX feature_proposals_project ON feature_proposals(project_id,status,id);
CREATE INDEX task_feature_links_feature ON task_feature_links(project_id,feature_id);
-- Use the authorized document-scope invalidation stream; no proposal contents are broadcast.
CREATE TRIGGER features_live_update AFTER INSERT OR UPDATE OR DELETE ON project_features FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
CREATE TRIGGER feature_proposals_live_update AFTER INSERT OR UPDATE OR DELETE ON feature_proposals FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
CREATE TRIGGER requirement_features_live_update AFTER INSERT OR UPDATE OR DELETE ON requirement_features FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
