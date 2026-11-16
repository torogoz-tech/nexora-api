CREATE TABLE team_calendar_activities (
 id TEXT PRIMARY KEY, created_by TEXT NOT NULL REFERENCES actors(id),
 title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 200),
 description TEXT NOT NULL DEFAULT '', location TEXT NOT NULL DEFAULT '',
 start_date DATE NOT NULL, end_date DATE NOT NULL,
 all_day BOOLEAN NOT NULL, start_time TEXT, end_time TEXT,
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','cancelled')),
 revision BIGINT NOT NULL DEFAULT 0,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 CHECK(end_date>=start_date AND end_date-start_date<=365),
 CHECK((all_day AND start_time IS NULL AND end_time IS NULL) OR
 (NOT all_day AND start_time IS NOT NULL AND end_time IS NOT NULL AND
 start_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$' AND end_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$' AND
 (end_date>start_date OR end_time>start_time)))
);
CREATE INDEX team_calendar_activities_dates ON team_calendar_activities(start_date,end_date) WHERE status='active';
