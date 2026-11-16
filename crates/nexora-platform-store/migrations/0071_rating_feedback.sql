CREATE TABLE developer_rating_feedback (
 actor_id text NOT NULL REFERENCES users(actor_id),
 year integer NOT NULL CHECK(year BETWEEN 2000 AND 9998),
 report jsonb,
 requested_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 generated_at timestamptz,
 PRIMARY KEY(actor_id,year)
);
