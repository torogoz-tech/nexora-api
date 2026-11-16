ALTER TABLE labor_calendar_settings ADD COLUMN work_start text NOT NULL DEFAULT '09:00', ADD COLUMN work_end text NOT NULL DEFAULT '17:00', ADD COLUMN break_minutes integer NOT NULL DEFAULT 0;
UPDATE labor_calendar_settings SET work_end=to_char(time '09:00'+daily_minutes*interval '1 minute','HH24:MI');
ALTER TABLE labor_calendar_settings ADD CONSTRAINT labor_working_hours_valid CHECK(
 work_start ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$' AND work_end ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'
 AND break_minutes>=0 AND daily_minutes=(extract(epoch FROM (work_end::time-work_start::time))/60)::integer+CASE WHEN work_end<=work_start THEN 1440 ELSE 0 END-break_minutes
);
CREATE TABLE labor_overtime_requests(
 id text PRIMARY KEY,actor_id text NOT NULL REFERENCES actors(id),date date NOT NULL,timezone text NOT NULL,
 start_time text NOT NULL CHECK(start_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'),
 end_time text NOT NULL CHECK(end_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'),next_day boolean NOT NULL DEFAULT false,
 minutes integer NOT NULL CHECK(minutes BETWEEN 1 AND 1440),reason text NOT NULL,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
 decision_reason text,decided_by text REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),decided_at timestamptz,
 CHECK(minutes=(extract(epoch FROM(end_time::time-start_time::time))/60)::integer+CASE WHEN next_day THEN 1440 ELSE 0 END)
);
CREATE INDEX labor_overtime_actor_date ON labor_overtime_requests(actor_id,date) WHERE status IN ('pending','approved');
