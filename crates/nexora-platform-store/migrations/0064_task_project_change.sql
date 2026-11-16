-- Scope follows a deliberately transferred task. Content, authorship and timestamps stay immutable.
ALTER TABLE task_history DROP CONSTRAINT task_history_task_id_project_id_fkey;
ALTER TABLE task_history ADD CONSTRAINT task_history_task_id_project_id_fkey FOREIGN KEY(task_id,project_id) REFERENCES tasks(id,project_id) ON UPDATE CASCADE;
ALTER TABLE task_comments DROP CONSTRAINT task_comments_task_id_project_id_fkey;
ALTER TABLE task_comments ADD CONSTRAINT task_comments_task_id_project_id_fkey FOREIGN KEY(task_id,project_id) REFERENCES tasks(id,project_id) ON UPDATE CASCADE;
CREATE OR REPLACE FUNCTION forbid_task_comment_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND (to_jsonb(NEW)-'project_id')=(to_jsonb(OLD)-'project_id')
 AND EXISTS(SELECT 1 FROM tasks t WHERE t.id=NEW.task_id AND t.project_id IS NOT DISTINCT FROM NEW.project_id) THEN RETURN NEW; END IF;
 RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task comments are immutable contributions';
END $$;
