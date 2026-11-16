ALTER TABLE task_dependencies
 ADD COLUMN resolved_at timestamptz,
 ADD COLUMN resolved_by text REFERENCES actors(id),
 ADD COLUMN resolution_reason text CHECK(octet_length(resolution_reason) BETWEEN 1 AND 4000),
 ADD COLUMN removed_at timestamptz,
 ADD COLUMN removed_by text REFERENCES actors(id),
 ADD COLUMN removal_reason text CHECK(octet_length(removal_reason) BETWEEN 1 AND 4000),
 ADD CONSTRAINT dependency_removal_metadata CHECK(
  (removed_at IS NULL AND removed_by IS NULL AND removal_reason IS NULL) OR
  (removed_at IS NOT NULL AND removed_by IS NOT NULL AND removal_reason IS NOT NULL)
 );
-- Existing resolved rows retain unknown historical attribution rather than inventing it.
DROP INDEX task_dependency_active;
CREATE UNIQUE INDEX task_dependency_active ON task_dependencies(project_id,source_task_id,target_task_id,kind)
 WHERE status='active' AND removed_at IS NULL;
ALTER TABLE tasks ADD COLUMN comment_sequence bigint NOT NULL DEFAULT 0 CHECK(comment_sequence>=0);
CREATE TABLE task_comments (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 task_id text NOT NULL,
 sequence bigint NOT NULL CHECK(sequence>0),
 body text NOT NULL CHECK(octet_length(body) BETWEEN 1 AND 8000),
 actor_id text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(task_id,project_id) REFERENCES tasks(id,project_id),
 UNIQUE(task_id,sequence)
);
CREATE INDEX task_comments_page ON task_comments(project_id,task_id,sequence);
CREATE FUNCTION forbid_task_comment_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task comments are immutable contributions';
END;
$$;
CREATE TRIGGER task_comment_immutable BEFORE UPDATE OR DELETE ON task_comments
 FOR EACH ROW EXECUTE FUNCTION forbid_task_comment_mutation();
