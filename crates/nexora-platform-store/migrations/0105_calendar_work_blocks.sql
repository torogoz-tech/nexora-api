ALTER TABLE task_work_logs ALTER COLUMN task_id DROP NOT NULL;
ALTER TABLE task_work_logs ADD COLUMN title text, ADD COLUMN start_time text, ADD COLUMN end_time text;
ALTER TABLE task_work_logs ADD CONSTRAINT work_block_shape CHECK (
 (title IS NULL AND start_time IS NULL AND end_time IS NULL AND task_id IS NOT NULL) OR
 (title IS NOT NULL AND length(trim(title)) BETWEEN 1 AND 200 AND start_time IS NOT NULL AND end_time IS NOT NULL
 AND start_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$' AND end_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'
 AND end_time>start_time AND minutes=(extract(epoch FROM (end_time::time-start_time::time))/60)::integer));
CREATE FUNCTION member_daily_availability(member text,day date) RETURNS jsonb LANGUAGE sql STABLE AS $$
 WITH config AS (SELECT *, day+work_start::time AS begins,day+work_end::time+CASE WHEN work_end<=work_start THEN interval '1 day' ELSE interval '0' END AS ends FROM labor_calendar_settings WHERE singleton),
 scheduled AS (SELECT *,CASE WHEN extract(isodow FROM day)::integer=ANY(workdays) AND NOT EXISTS(SELECT 1 FROM labor_holidays h WHERE h.date=day AND h.status='approved' AND (h.country IS NULL OR h.country=config.country)) THEN daily_minutes ELSE 0 END AS scheduled FROM config),
 absences AS (SELECT s.*,LEAST(s.scheduled,COALESCE((SELECT sum(COALESCE(l.minutes_per_day,s.daily_minutes)) FROM labor_leave_requests l WHERE l.actor_id=member AND l.status='approved' AND day BETWEEN l.start_date AND l.end_date),0))::integer AS absence FROM scheduled s),
 periods AS (
 SELECT tsrange(GREATEST(s.begins,e.start_date+COALESCE(e.start_time,'00:00')::time),LEAST(s.ends,CASE WHEN e.all_day THEN e.end_date+interval '1 day' ELSE e.end_date+e.end_time::time END),'[)') AS span
 FROM absences s JOIN team_calendar_activities e ON e.status='active' AND e.start_date<=s.ends::date AND e.end_date>=day
 WHERE (NOT e.attendance_required OR EXISTS(SELECT 1 FROM calendar_activity_attendance a WHERE a.activity_id=e.id AND a.actor_id=member AND a.activity_revision=e.revision AND a.response='attending'))
 AND NOT EXISTS(SELECT 1 FROM calendar_activity_attendance a WHERE a.activity_id=e.id AND a.actor_id=member AND a.activity_revision=e.revision AND a.response='declined')
 AND e.start_date+COALESCE(e.start_time,'00:00')::time<s.ends AND CASE WHEN e.all_day THEN e.end_date+interval '1 day' ELSE e.end_date+e.end_time::time END>s.begins
 UNION ALL
 SELECT tsrange(GREATEST(s.begins,w.date+w.start_time::time),LEAST(s.ends,w.date+w.end_time::time),'[)') FROM absences s JOIN task_work_logs w ON w.actor_id=member AND w.date BETWEEN day AND s.ends::date AND w.task_id IS NULL AND w.removed_at IS NULL AND w.overtime_request_id IS NULL
 WHERE w.date+w.start_time::time<s.ends AND w.date+w.end_time::time>s.begins
 ), merged AS (SELECT range_agg(span) spans FROM periods), deduction AS (SELECT COALESCE(sum(extract(epoch FROM (upper(span)-lower(span)))/60),0)::integer AS minutes FROM merged,unnest(spans) span)
 SELECT jsonb_build_object('scheduled_minutes',scheduled,'absence_minutes',absence,'activity_minutes',LEAST(GREATEST(scheduled-absence,0),minutes),'expected_task_minutes',GREATEST(scheduled-absence-minutes,0)) FROM absences,deduction
$$;
CREATE OR REPLACE FUNCTION developer_rating_excluded(member text,day date) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT (member_daily_availability(member,day)->>'expected_task_minutes')::integer=0
$$;
CREATE FUNCTION calendar_work_dirty() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO developer_rating_dirty(actor_id) SELECT actor_id FROM actor_roles WHERE role IN ('developer','technical_lead') ON CONFLICT DO NOTHING;
 RETURN NULL;
END $$;
CREATE FUNCTION work_block_dirty() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP <> 'INSERT' THEN INSERT INTO developer_rating_dirty(actor_id) VALUES(OLD.actor_id) ON CONFLICT DO NOTHING; END IF;
 IF TG_OP <> 'DELETE' THEN INSERT INTO developer_rating_dirty(actor_id) VALUES(NEW.actor_id) ON CONFLICT DO NOTHING; END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER work_block_rating_dirty AFTER INSERT OR UPDATE OR DELETE ON task_work_logs FOR EACH ROW EXECUTE FUNCTION work_block_dirty();
CREATE TRIGGER team_activity_rating_dirty AFTER INSERT OR UPDATE OR DELETE ON team_calendar_activities FOR EACH STATEMENT EXECUTE FUNCTION calendar_work_dirty();
CREATE TRIGGER attendance_rating_dirty AFTER INSERT OR UPDATE OR DELETE ON calendar_activity_attendance FOR EACH STATEMENT EXECUTE FUNCTION calendar_work_dirty();
