CREATE TABLE labor_calendar_settings (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 timezone text NOT NULL DEFAULT 'UTC',
 country text CHECK(country IS NULL OR country ~ '^[A-Z]{2}$'),
 workdays integer[] NOT NULL DEFAULT ARRAY[1,2,3,4,5] CHECK(cardinality(workdays)>0 AND workdays <@ ARRAY[1,2,3,4,5,6,7]),
 daily_minutes integer NOT NULL DEFAULT 480 CHECK(daily_minutes BETWEEN 1 AND 1440),
 starts_on date NOT NULL DEFAULT CURRENT_DATE,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0)
);
INSERT INTO labor_calendar_settings(singleton) VALUES(true);
CREATE TABLE labor_holidays (
 id text PRIMARY KEY,name text NOT NULL,date date NOT NULL,country text CHECK(country IS NULL OR country ~ '^[A-Z]{2}$'),
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
 proposed_by text NOT NULL REFERENCES actors(id),reason text NOT NULL,
 decision_reason text,decided_by text REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),decided_at timestamptz
);
CREATE UNIQUE INDEX labor_holidays_approved_date ON labor_holidays(date,COALESCE(country,'')) WHERE status='approved';
CREATE TABLE labor_leave_requests (
 id text PRIMARY KEY,actor_id text NOT NULL REFERENCES actors(id),kind text NOT NULL CHECK(kind IN ('vacation','leave')),
 start_date date NOT NULL,end_date date NOT NULL CHECK(end_date>=start_date),minutes_per_day integer CHECK(minutes_per_day BETWEEN 1 AND 1440),
 reason text NOT NULL,status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
 decision_reason text,decided_by text REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),decided_at timestamptz,
 CHECK(kind<>'vacation' OR minutes_per_day IS NULL)
);
CREATE INDEX labor_leave_requests_actor_dates ON labor_leave_requests(actor_id,start_date,end_date);
CREATE TABLE labor_approvers (
 actor_id text PRIMARY KEY REFERENCES actors(id),approver_id text REFERENCES actors(id),revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 CHECK(actor_id<>approver_id)
);
