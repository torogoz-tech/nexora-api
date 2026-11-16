CREATE TABLE git_author_aliases (
 id text PRIMARY KEY, actor_id text NOT NULL REFERENCES actors(id), provider text NOT NULL CHECK(provider IN ('github','gitlab')),
 host text NOT NULL,email text NOT NULL,project_id text REFERENCES projects(id), revision bigint NOT NULL DEFAULT 1,
 created_by text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 revoked_at timestamptz,revoked_by text REFERENCES actors(id),revocation_reason text
);
CREATE INDEX git_author_alias_identity ON git_author_aliases(provider,host,email) WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX git_author_alias_scope ON git_author_aliases(provider,host,email,COALESCE(project_id,'')) WHERE revoked_at IS NULL;
CREATE TABLE git_commit_authorship (
 id text PRIMARY KEY,provider text NOT NULL CHECK(provider IN ('github','gitlab')),host text NOT NULL,
 remote_repository_id text NOT NULL,repository_id text NOT NULL REFERENCES project_repositories(id),project_id text NOT NULL REFERENCES projects(id),
 commit_sha text NOT NULL,author_name text NOT NULL,author_email text,author_email_raw text,
 actor_id text REFERENCES actors(id),alias_id text REFERENCES git_author_aliases(id),
 scope_id text NOT NULL,delivery_id text NOT NULL,occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),attributed_at timestamptz,
 CHECK((actor_id IS NULL)=(alias_id IS NULL)),
 UNIQUE(provider,host,remote_repository_id,commit_sha,project_id)
);
CREATE INDEX git_author_unmatched ON git_commit_authorship(provider,host,author_email,project_id) WHERE actor_id IS NULL;
CREATE INDEX git_author_activity ON git_commit_authorship(actor_id,occurred_at);
CREATE FUNCTION git_author_alias_scope_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended(current_schema()||':git-author-aliases',0));
 IF NEW.revoked_at IS NULL AND EXISTS(SELECT 1 FROM git_author_aliases a WHERE a.id<>NEW.id AND a.provider=NEW.provider AND a.host=NEW.host AND a.email=NEW.email AND a.revoked_at IS NULL AND (a.project_id IS NULL OR NEW.project_id IS NULL OR a.project_id=NEW.project_id)) THEN
 RAISE EXCEPTION USING ERRCODE='23505',MESSAGE='author alias already bound in overlapping scope'; END IF; RETURN NEW; END $$;
CREATE TRIGGER git_author_alias_scope BEFORE INSERT OR UPDATE ON git_author_aliases FOR EACH ROW EXECUTE FUNCTION git_author_alias_scope_guard();
