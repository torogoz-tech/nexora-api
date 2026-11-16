CREATE TABLE organization_documents (
 id text PRIMARY KEY,kind text NOT NULL CHECK(kind IN ('document','requirement','business_rule','decision','best_practice','technical_debt')),
 title text NOT NULL,body text NOT NULL,revision bigint NOT NULL DEFAULT 1,
 created_by text NOT NULL REFERENCES actors(id),updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE organization_document_versions (
 document_id text NOT NULL REFERENCES organization_documents(id),revision bigint NOT NULL,
 title text NOT NULL,body text NOT NULL,created_by text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(document_id,revision)
);

CREATE FUNCTION organization_knowledge_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO live_update_revisions(scope_id,topic,revision) VALUES('*','documents',1) ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 RETURN NEW;
END;$$;
CREATE TRIGGER organization_knowledge_live AFTER INSERT OR UPDATE ON organization_documents FOR EACH ROW EXECUTE FUNCTION organization_knowledge_changed();

INSERT INTO actor_grants(actor_id,capability,project_id)
 SELECT actor_id,'organization-knowledge:read',NULL FROM actor_roles r
 WHERE role IN ('developer','technical_lead','pmo','manager','super_admin')
 AND NOT EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability='organization-knowledge:read' AND g.project_id IS NULL);
INSERT INTO actor_grants(actor_id,capability,project_id)
 SELECT actor_id,'organization-knowledge:write',NULL FROM actor_roles r
 WHERE role IN ('technical_lead','pmo','manager','super_admin')
 AND NOT EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability='organization-knowledge:write' AND g.project_id IS NULL);
