-- Business-facing catalog; technical mappings never form part of the request projection.
CREATE TABLE products (
 id text PRIMARY KEY, name text NOT NULL CHECK(length(btrim(name)) BETWEEN 1 AND 120),
 description text NOT NULL DEFAULT '', archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0), created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX product_name_unique ON products(lower(btrim(name)));
CREATE TABLE product_projects (
 product_id text NOT NULL REFERENCES products(id), project_id text NOT NULL REFERENCES projects(id),
 PRIMARY KEY(product_id,project_id)
);
CREATE INDEX product_projects_project ON product_projects(project_id,product_id);
ALTER TABLE service_requests ADD COLUMN product_id text REFERENCES products(id);
CREATE INDEX service_requests_product ON service_requests(product_id);

-- Reviews are independent of task/document workflow states. Preserve all historical references.
CREATE TABLE knowledge_reviews (
 id text PRIMARY KEY, kind text NOT NULL CHECK(kind IN ('project_document','organization_document','contribution','feature')),
 resource_id text NOT NULL, project_id text REFERENCES projects(id), resource_revision bigint NOT NULL CHECK(resource_revision>0),
 version_id text, title text NOT NULL, document_kind text NOT NULL,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
 requested_by text NOT NULL REFERENCES actors(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 decided_by text REFERENCES actors(id), decided_at timestamptz, reason text NOT NULL DEFAULT '',
 revision bigint NOT NULL DEFAULT 1,
 CHECK((status='pending')=(decided_by IS NULL AND decided_at IS NULL))
);
CREATE UNIQUE INDEX knowledge_review_pending ON knowledge_reviews(kind,resource_id,resource_revision) WHERE status='pending';
CREATE INDEX knowledge_reviews_queue ON knowledge_reviews(status,created_at,id);
ALTER TABLE organization_documents ADD COLUMN published_revision bigint;
ALTER TABLE organization_documents ADD FOREIGN KEY(id,published_revision) REFERENCES organization_document_versions(document_id,revision);
-- Existing organization knowledge was immediately visible; preserve its published baseline.
UPDATE organization_documents SET published_revision=revision;

CREATE FUNCTION notify_knowledge_review() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
  SELECT 'knowledge_review:'||NEW.id||':'||u.actor_id,u.actor_id,'knowledge_review',NEW.project_id,NEW.id
  FROM users u JOIN actors a ON a.id=u.actor_id AND a.active JOIN actor_roles r ON r.actor_id=u.actor_id
  WHERE r.role IN ('technical_lead','pmo','manager','super_admin')
  AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.capability=CASE WHEN NEW.project_id IS NULL THEN 'organization-knowledge:write' ELSE 'approvals:grant' END AND (g.project_id IS NULL OR g.project_id=NEW.project_id))
  ON CONFLICT DO NOTHING;
 ELSIF NEW.status IS DISTINCT FROM OLD.status THEN
  INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
  SELECT 'knowledge_result:'||NEW.id||':'||NEW.revision,NEW.requested_by,'knowledge_result',NEW.project_id,NEW.id
  FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id=NEW.requested_by
  ON CONFLICT DO NOTHING;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER knowledge_review_inbox AFTER INSERT OR UPDATE OF status ON knowledge_reviews FOR EACH ROW EXECUTE FUNCTION notify_knowledge_review();
