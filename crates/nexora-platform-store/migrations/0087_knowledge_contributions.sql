-- Contribute is deliberately separate from curated knowledge editing.
INSERT INTO actor_grants(actor_id,capability,project_id)
 SELECT r.actor_id,c.capability,NULL FROM actor_roles r
 CROSS JOIN (VALUES ('knowledge:contribute'),('analysis-profiles:read'),('analysis-profiles:contribute')) c(capability)
 WHERE r.role IN ('developer','technical_lead','pmo','manager','super_admin')
 AND NOT EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability=c.capability AND g.project_id IS NULL);
DELETE FROM actor_grants g USING actor_roles r
 WHERE g.actor_id=r.actor_id AND r.role='developer' AND g.capability IN ('documents:write','knowledge:write');
INSERT INTO actor_grants(actor_id,capability,project_id)
 SELECT g.actor_id,'knowledge:write',g.project_id FROM actor_grants g JOIN actor_roles r ON r.actor_id=g.actor_id
 WHERE g.capability='knowledge:read' AND r.role IN ('technical_lead','pmo','manager')
 AND NOT EXISTS(SELECT 1 FROM actor_grants w WHERE w.actor_id=g.actor_id AND w.capability='knowledge:write' AND w.project_id IS NOT DISTINCT FROM g.project_id);
CREATE TABLE knowledge_contributions (
 id text PRIMARY KEY, project_id text REFERENCES projects(id),
 kind text NOT NULL CHECK(kind IN ('document','technical_debt')),
 source_revision bigint NOT NULL DEFAULT 1, title text NOT NULL, body text NOT NULL DEFAULT '', revision bigint NOT NULL DEFAULT 1,
 filename text, mime_type text, payload bytea, sha256 text,
 status text NOT NULL CHECK(status IN ('queued','processing','ready','failed')),
 attempts integer NOT NULL DEFAULT 0, claimed_at timestamptz,
 created_by text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),
 CHECK((kind='document' AND filename IS NOT NULL AND payload IS NOT NULL) OR (kind='technical_debt' AND payload IS NULL))
);
CREATE INDEX knowledge_contribution_page ON knowledge_contributions(project_id,kind,id);
CREATE TABLE knowledge_contribution_versions (
 contribution_id text NOT NULL REFERENCES knowledge_contributions(id),revision bigint NOT NULL,
 source_revision bigint NOT NULL, title text NOT NULL,body text NOT NULL,created_by text REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(contribution_id,revision)
);
CREATE FUNCTION knowledge_contribution_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO live_update_revisions(scope_id,topic,revision) VALUES(COALESCE(NEW.project_id,'*'),'documents',1)
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 RETURN NEW;
END;$$;
CREATE TRIGGER knowledge_contribution_live AFTER INSERT OR UPDATE ON knowledge_contributions FOR EACH ROW EXECUTE FUNCTION knowledge_contribution_changed();

CREATE TABLE knowledge_contribution_sources (
 contribution_id text NOT NULL REFERENCES knowledge_contributions(id),source_revision bigint NOT NULL,
 filename text NOT NULL,mime_type text NOT NULL,payload bytea NOT NULL,sha256 text NOT NULL,
 created_by text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(contribution_id,source_revision)
);
