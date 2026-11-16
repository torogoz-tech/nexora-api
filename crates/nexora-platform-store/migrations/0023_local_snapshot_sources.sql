-- Local imports have their own authority ledger; Git bindings remain unchanged.
ALTER TABLE project_repositories
 ADD COLUMN source_kind text NOT NULL DEFAULT 'git' CHECK(source_kind IN ('git','local_snapshot')),
 ADD COLUMN local_name text CHECK(local_name IS NULL OR octet_length(local_name) BETWEEN 1 AND 160),
 ALTER COLUMN provider DROP NOT NULL,
 ALTER COLUMN host DROP NOT NULL,
 ALTER COLUMN requested_full_name DROP NOT NULL,
 DROP CONSTRAINT project_repositories_check,
 ADD CONSTRAINT repository_source_metadata CHECK(
  (source_kind='git' AND provider IS NOT NULL AND host IS NOT NULL AND requested_full_name IS NOT NULL AND local_name IS NULL
   AND (authorization_status<>'authorized' OR (remote_id IS NOT NULL AND remote_full_name IS NOT NULL AND connection_id IS NOT NULL)))
  OR
  (source_kind='local_snapshot' AND local_name IS NOT NULL AND provider IS NULL AND host IS NULL AND requested_full_name IS NULL
   AND remote_id IS NULL AND remote_full_name IS NULL AND connection_id IS NULL AND latest_source_commit IS NULL)
 );

CREATE FUNCTION retain_repository_source_kind() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.source_kind IS DISTINCT FROM OLD.source_kind THEN
  RAISE EXCEPTION 'repository source kind is immutable' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END;
$$;


CREATE TRIGGER repository_source_kind_immutable BEFORE UPDATE ON project_repositories
 FOR EACH ROW EXECUTE FUNCTION retain_repository_source_kind();

CREATE TABLE local_repository_sources(
 id text PRIMARY KEY CHECK(id ~ '^lsrc_[0-9a-f]{32}$'),
 repository_id text NOT NULL UNIQUE,
 project_id text NOT NULL,
 runtime_tenant_id text NOT NULL CHECK(octet_length(runtime_tenant_id) BETWEEN 1 AND 4096),
 runtime_repository_id text NOT NULL CHECK(runtime_repository_id ~ '^[0-9a-f]{64}$'),
 owner_actor_id text NOT NULL REFERENCES actors(id),
 state text NOT NULL DEFAULT 'active' CHECK(state IN ('active','revoked')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 authorization_revision bigint NOT NULL DEFAULT 1 CHECK(authorization_revision>0),
 source_generation bigint NOT NULL DEFAULT 0 CHECK(source_generation>=0 AND authorization_revision=source_generation+1),
 binding_revision bigint NOT NULL DEFAULT 1 CHECK(binding_revision>0),
 current_publication_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(repository_id,project_id) REFERENCES project_repositories(id,project_id),
 UNIQUE(runtime_tenant_id,runtime_repository_id),
 CHECK(revision>=authorization_revision AND revision>=binding_revision)
);
CREATE INDEX local_repository_source_project ON local_repository_sources(project_id,repository_id);

CREATE TABLE local_snapshot_publications(
 id text PRIMARY KEY CHECK(id ~ '^lpub_[0-9a-f]{32}$'),
 source_id text NOT NULL REFERENCES local_repository_sources(id),
 authorization_revision bigint NOT NULL CHECK(authorization_revision>0),
 snapshot_id text NOT NULL CHECK(octet_length(snapshot_id) BETWEEN 1 AND 4096),
 payload_hash text NOT NULL CHECK(payload_hash ~ '^[0-9a-f]{64}$'),
 binding_revision bigint NOT NULL CHECK(binding_revision>0),
 attested_by text NOT NULL REFERENCES actors(id),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 published_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(source_id,id),
 UNIQUE(source_id,binding_revision)
);
CREATE INDEX local_publication_snapshot ON local_snapshot_publications(source_id,authorization_revision,snapshot_id);
ALTER TABLE local_repository_sources ADD CONSTRAINT local_current_publication
 FOREIGN KEY(id,current_publication_id) REFERENCES local_snapshot_publications(source_id,id)
 DEFERRABLE INITIALLY DEFERRED;
CREATE TRIGGER local_publication_immutable BEFORE UPDATE OR DELETE ON local_snapshot_publications
 FOR EACH ROW EXECUTE FUNCTION forbid_document_record_mutation();

CREATE FUNCTION retain_local_source_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF NOT EXISTS(SELECT 1 FROM project_repositories WHERE id=NEW.repository_id AND project_id=NEW.project_id AND source_kind='local_snapshot') THEN
   RAISE EXCEPTION 'local source requires a local repository' USING ERRCODE='23514';
  END IF;
 ELSE
  IF (NEW.id,NEW.repository_id,NEW.project_id,NEW.runtime_tenant_id,NEW.runtime_repository_id,NEW.owner_actor_id,NEW.created_at)
   IS DISTINCT FROM
   (OLD.id,OLD.repository_id,OLD.project_id,OLD.runtime_tenant_id,OLD.runtime_repository_id,OLD.owner_actor_id,OLD.created_at) THEN
   RAISE EXCEPTION 'local source identity is immutable' USING ERRCODE='23514';
  END IF;
  IF NEW.revision<>OLD.revision+1 OR NEW.authorization_revision<OLD.authorization_revision
   OR NEW.authorization_revision>OLD.authorization_revision+1
   OR ((NEW.state IS DISTINCT FROM OLD.state) IS DISTINCT FROM (NEW.authorization_revision=OLD.authorization_revision+1))
   OR NEW.binding_revision<>OLD.binding_revision+(CASE WHEN NEW.current_publication_id IS DISTINCT FROM OLD.current_publication_id THEN 1 ELSE 0 END) THEN
   RAISE EXCEPTION 'invalid local source revision transition' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER local_source_identity_immutable BEFORE INSERT OR UPDATE ON local_repository_sources
 FOR EACH ROW EXECUTE FUNCTION retain_local_source_identity();

-- Public metadata excludes the root and legacy runtime namespace.
CREATE FUNCTION repository_metadata(r project_repositories) RETURNS jsonb LANGUAGE SQL STABLE AS $$
 SELECT to_jsonb(r) || CASE WHEN r.source_kind='local_snapshot' THEN
  jsonb_build_object('local_source',(
   SELECT jsonb_build_object('source_id',s.id,'source_revision',s.revision,
    'authorization_revision',s.authorization_revision,'source_generation',s.source_generation,
    'state',s.state,'owner_actor_id',s.owner_actor_id,'binding_revision',s.binding_revision,
    'publication_id',p.id,'snapshot_id',p.snapshot_id,'payload_hash',p.payload_hash)
   FROM local_repository_sources s JOIN local_snapshot_publications p ON p.source_id=s.id AND p.id=s.current_publication_id
   WHERE s.repository_id=r.id AND s.project_id=r.project_id
  )) ELSE '{}'::jsonb END;
$$;

-- Preserve the existing status response and Git semantics; local trust is independent.
CREATE OR REPLACE FUNCTION project_context_status(p_project text,p_manifest text) RETURNS jsonb LANGUAGE SQL STABLE AS $$
 WITH members AS (
  SELECT item FROM project_context_manifests m CROSS JOIN LATERAL jsonb_array_elements(m.members) item
  WHERE m.id=p_manifest AND m.project_id=p_project
 ), sources AS (
  SELECT item,r.source_kind,
   CASE WHEN r.source_kind='local_snapshot' THEN lp.snapshot_id ELSE b.snapshot_id END AS snapshot_id,
   CASE WHEN r.source_kind='local_snapshot' THEN lp.payload_hash ELSE b.payload_hash END AS payload_hash,
   CASE WHEN r.source_kind='local_snapshot' THEN ls.source_generation ELSE b.source_generation END AS source_generation,
   b.pinned_commit,b.connection_id,b.connection_revision,lp.id AS local_publication_id,
   CASE WHEN r.source_kind='local_snapshot' THEN
    COALESCE(ls.id IS NOT NULL AND ls.state='active' AND la.active AND NOT r.archived AND r.authorization_status='authorized'
     AND r.source_generation=ls.source_generation AND lp.authorization_revision=ls.authorization_revision
     AND item#>>'{local_origin,source_id}'=ls.id
     AND (item#>>'{local_origin,authorization_revision}')::bigint=ls.authorization_revision
     AND proof.id IS NOT NULL,false)
   ELSE
    COALESCE(b.repository_id IS NOT NULL AND r.id IS NOT NULL AND NOT r.archived AND r.authorization_status='authorized'
     AND r.source_generation=b.source_generation AND r.connection_id=b.connection_id AND c.status='active'
     AND c.revision=b.connection_revision AND ga.active AND i.singleton AND b.runtime_repository_id=b.repository_id,false)
   END AS available
  FROM members
  LEFT JOIN project_repositories r ON r.id=item->>'repository_id' AND r.project_id=p_project
  LEFT JOIN repository_bindings b ON b.repository_id=r.id AND b.project_id=p_project AND r.source_kind='git'
  LEFT JOIN git_connections c ON c.id=b.connection_id
  LEFT JOIN actors ga ON ga.id=c.owner_actor_id
  LEFT JOIN installation i ON i.id=b.runtime_tenant_id
  LEFT JOIN local_repository_sources ls ON ls.repository_id=r.id AND ls.project_id=p_project AND r.source_kind='local_snapshot'
  LEFT JOIN actors la ON la.id=ls.owner_actor_id
  LEFT JOIN local_snapshot_publications lp ON lp.source_id=ls.id AND lp.id=ls.current_publication_id
  LEFT JOIN local_snapshot_publications proof ON proof.source_id=ls.id AND proof.id=item#>>'{local_origin,publication_id}'
   AND proof.authorization_revision=ls.authorization_revision AND proof.snapshot_id=item->>'snapshot_id' AND proof.payload_hash=item->>'payload_hash'
 ), classified AS (
  SELECT item->>'repository_id' AS repository_id,NOT available AS unavailable,
   available AND (snapshot_id IS DISTINCT FROM item->>'snapshot_id'
    OR payload_hash IS DISTINCT FROM item->>'payload_hash'
    OR source_generation IS DISTINCT FROM (item->>'source_generation')::bigint
    OR CASE WHEN source_kind='local_snapshot' THEN local_publication_id IS DISTINCT FROM item#>>'{local_origin,publication_id}'
       ELSE pinned_commit IS DISTINCT FROM item->>'pinned_commit'
        OR connection_id IS DISTINCT FROM item->>'connection_id'
        OR connection_revision IS DISTINCT FROM (item->>'connection_revision')::bigint END) AS changed
  FROM sources
 )
 SELECT jsonb_build_object('manifest_id',p_manifest,'needs_review',COALESCE(bool_or(unavailable OR changed),false),
  'changed_repository_ids',COALESCE(jsonb_agg(repository_id ORDER BY repository_id) FILTER(WHERE changed),'[]'::jsonb),
  'unavailable_repository_ids',COALESCE(jsonb_agg(repository_id ORDER BY repository_id) FILTER(WHERE unavailable),'[]'::jsonb)) FROM classified;
$$;
