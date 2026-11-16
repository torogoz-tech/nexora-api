-- Coalesced invalidations, not event payloads: clients always reread authorized GETs.
CREATE TABLE live_update_revisions (
 scope_id text NOT NULL, topic text NOT NULL, revision bigint NOT NULL DEFAULT 1,
 PRIMARY KEY(scope_id,topic)
);
CREATE FUNCTION signal_project_live_update() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE scope_value text;
BEGIN
 scope_value := CASE WHEN TG_ARGV[1]='global' THEN '*' ELSE COALESCE(to_jsonb(NEW)->>'project_id',to_jsonb(OLD)->>'project_id') END;
 IF scope_value IS NULL AND TG_TABLE_NAME='document_version_states' THEN
 SELECT project_id INTO scope_value FROM project_document_versions WHERE id=COALESCE(to_jsonb(NEW)->>'version_id',to_jsonb(OLD)->>'version_id');
 END IF;
 IF scope_value IS NOT NULL THEN
 INSERT INTO live_update_revisions(scope_id,topic) VALUES(scope_value,TG_ARGV[0])
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER documents_live_update AFTER INSERT OR UPDATE OR DELETE ON project_documents FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
CREATE TRIGGER document_versions_live_update AFTER INSERT OR UPDATE OR DELETE ON project_document_versions FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
CREATE TRIGGER document_publications_live_update AFTER INSERT OR UPDATE OR DELETE ON document_publications FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
CREATE TRIGGER repositories_live_update AFTER INSERT OR UPDATE OR DELETE ON project_repositories FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('repositories','project');
CREATE TRIGGER catalog_live_update AFTER INSERT OR UPDATE OR DELETE ON task_type_catalog FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('task-types','global');
CREATE FUNCTION signal_knowledge_live_update() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE scope_value text;
BEGIN
 IF NEW.operation LIKE 'knowledge.%' THEN
 scope_value:=NEW.metadata->>'project_id';
 IF scope_value IS NOT NULL THEN
 INSERT INTO live_update_revisions(scope_id,topic) VALUES(scope_value,'knowledge')
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 END IF;
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER knowledge_live_update AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION signal_knowledge_live_update();

CREATE TRIGGER document_states_live_update AFTER INSERT OR UPDATE OR DELETE ON document_version_states FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
CREATE TRIGGER document_links_live_update AFTER INSERT OR UPDATE OR DELETE ON document_links FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('documents','project');
