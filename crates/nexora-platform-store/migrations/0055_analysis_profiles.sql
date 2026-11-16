CREATE TABLE analysis_profile_versions (
 id text PRIMARY KEY,key text NOT NULL,version text NOT NULL,name text NOT NULL,language_id text NOT NULL,
 checksum text NOT NULL,package jsonb NOT NULL,created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(key,version),
 CHECK(octet_length(package::text)<=20000000)
);
CREATE TABLE repository_analysis_profiles (
 repository_id text PRIMARY KEY REFERENCES project_repositories(id),
 profile_version_id text REFERENCES analysis_profile_versions(id),revision bigint NOT NULL CHECK(revision>0),
 changed_by text NOT NULL REFERENCES actors(id),changed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE repository_jobs ADD COLUMN analysis_profile_version_id text REFERENCES analysis_profile_versions(id);
CREATE FUNCTION preserve_analysis_profile_version() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='analysis profile versions are immutable'; END $$;
CREATE TRIGGER immutable_analysis_profile BEFORE UPDATE OR DELETE ON analysis_profile_versions FOR EACH ROW EXECUTE FUNCTION preserve_analysis_profile_version();
CREATE FUNCTION pin_repository_analysis_profile() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  PERFORM id FROM project_repositories WHERE id=NEW.repository_id FOR SHARE;
  SELECT profile_version_id INTO NEW.analysis_profile_version_id FROM repository_analysis_profiles WHERE repository_id=NEW.repository_id;
 ELSE
  IF NEW.analysis_profile_version_id IS DISTINCT FROM OLD.analysis_profile_version_id THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='job analysis profile is immutable';
  END IF;
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER repository_job_analysis_profile BEFORE INSERT OR UPDATE OF analysis_profile_version_id ON repository_jobs FOR EACH ROW EXECUTE FUNCTION pin_repository_analysis_profile();
