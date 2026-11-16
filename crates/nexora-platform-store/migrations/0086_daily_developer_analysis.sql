CREATE TABLE daily_developer_analyses (
 actor_id text NOT NULL REFERENCES actors(id),
 date date NOT NULL,
 timezone text NOT NULL,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','complete','no_evidence','failed')),
 evidence jsonb,
 report jsonb,
 lease_token text,
 started_at timestamptz,
 generated_at timestamptz,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(actor_id,date)
);
CREATE INDEX daily_developer_analyses_pending ON daily_developer_analyses(created_at) WHERE status='pending';
