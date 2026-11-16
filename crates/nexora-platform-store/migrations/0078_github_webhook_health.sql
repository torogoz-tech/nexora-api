CREATE TABLE github_webhook_health (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 configuration_revision bigint NOT NULL,
 status jsonb NOT NULL,
 checked_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
