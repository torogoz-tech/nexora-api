ALTER TABLE repository_analysis_profiles
 ADD COLUMN mode text NOT NULL DEFAULT 'legacy' CHECK(mode IN ('legacy','manual','automatic')),
 ADD COLUMN profile_version_ids jsonb NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(profile_version_ids)='array' AND jsonb_array_length(profile_version_ids)<=64);
UPDATE repository_analysis_profiles SET mode='manual',profile_version_ids=jsonb_build_array(profile_version_id) WHERE profile_version_id IS NOT NULL;
-- Absence after this migration means a genuinely new registration and defaults to automatic.
INSERT INTO repository_analysis_profiles(repository_id,profile_version_id,revision,changed_by,mode)
 SELECT r.id,NULL,1,(SELECT id FROM actors ORDER BY created_at,id LIMIT 1),'legacy' FROM project_repositories r
 WHERE NOT EXISTS(SELECT 1 FROM repository_analysis_profiles p WHERE p.repository_id=r.id);
ALTER TABLE repository_jobs ADD COLUMN analysis_profile_plan jsonb,
 ADD COLUMN analysis_profile_selection jsonb;
ALTER TABLE repository_jobs ADD CONSTRAINT repository_profile_plan_bound CHECK(analysis_profile_plan IS NULL OR octet_length(analysis_profile_plan::text)<=262144);
ALTER TABLE repository_jobs ADD CONSTRAINT repository_profile_selection_bound CHECK(analysis_profile_selection IS NULL OR octet_length(analysis_profile_selection::text)<=4194304);
CREATE OR REPLACE FUNCTION pin_repository_analysis_profile() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE selected_mode text; selected_ids jsonb; candidate_count bigint; candidate_bytes bigint;
BEGIN
 IF TG_OP='INSERT' THEN
  PERFORM id FROM project_repositories WHERE id=NEW.repository_id FOR SHARE;
  SELECT mode,profile_version_id,profile_version_ids INTO selected_mode,NEW.analysis_profile_version_id,selected_ids
   FROM repository_analysis_profiles WHERE repository_id=NEW.repository_id;
  selected_mode:=COALESCE(selected_mode,'automatic');
  IF selected_mode='legacy' THEN NEW.analysis_profile_plan:=NULL;
  ELSE
   -- Serialize against imports so only committed versions form this immutable set.
   PERFORM pg_advisory_xact_lock_shared(hashtextextended(current_schema()||':analysis-profile-import',0));
   IF selected_mode='automatic' THEN
    SELECT count(*),COALESCE(sum(octet_length(package::text)),0),COALESCE(jsonb_agg(id ORDER BY id),'[]'::jsonb)
     INTO candidate_count,candidate_bytes,selected_ids FROM analysis_profile_versions;
   ELSE
    SELECT count(*),COALESCE(sum(octet_length(package::text)),0) INTO candidate_count,candidate_bytes
     FROM analysis_profile_versions WHERE id IN(SELECT jsonb_array_elements_text(selected_ids));
    IF candidate_count<>jsonb_array_length(selected_ids) OR candidate_count=0 THEN
     RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='manual analysis profile selection is invalid';
    END IF;
   END IF;
   IF candidate_count>1024 OR candidate_bytes>67108864 THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='analysis profile candidate budget exceeded';
   END IF;
   NEW.analysis_profile_plan:=jsonb_build_object('mode',selected_mode,'policy',CASE WHEN selected_mode='automatic' THEN 'automatic-latest/v1' ELSE 'manual/v1' END,'profile_version_ids',selected_ids);
  END IF;
 ELSE
  IF NEW.analysis_profile_version_id IS DISTINCT FROM OLD.analysis_profile_version_id OR NEW.analysis_profile_plan IS DISTINCT FROM OLD.analysis_profile_plan THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='job analysis profile plan is immutable';
  END IF;
  IF OLD.analysis_profile_selection IS NOT NULL AND NEW.analysis_profile_selection IS DISTINCT FROM OLD.analysis_profile_selection THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='job analysis profile selection is immutable';
  END IF;
 END IF;RETURN NEW;
END $$;
DROP TRIGGER repository_job_analysis_profile ON repository_jobs;
CREATE TRIGGER repository_job_analysis_profile BEFORE INSERT OR UPDATE OF analysis_profile_version_id,analysis_profile_plan,analysis_profile_selection ON repository_jobs FOR EACH ROW EXECUTE FUNCTION pin_repository_analysis_profile();
CREATE FUNCTION guard_multi_profile_worker() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.analysis_profile_plan IS NOT NULL AND NEW.state='running' AND (OLD.state<>'running' OR NEW.attempt<>OLD.attempt)
  AND current_setting('nexora.worker_analysis_profiles',true) IS DISTINCT FROM '2' THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='multi-profile analysis requires an upgraded worker';
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER repository_job_multi_profile_worker BEFORE UPDATE OF state,attempt ON repository_jobs FOR EACH ROW EXECUTE FUNCTION guard_multi_profile_worker();
