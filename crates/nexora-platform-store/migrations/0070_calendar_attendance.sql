ALTER TABLE team_calendar_activities ADD COLUMN mode text CHECK(mode IS NULL OR mode IN ('virtual','in_person'));
ALTER TABLE team_calendar_activities ADD COLUMN attendance_required boolean NOT NULL DEFAULT false;
CREATE TABLE calendar_activity_attendance(
 activity_id text NOT NULL REFERENCES team_calendar_activities(id),
 actor_id text NOT NULL REFERENCES users(actor_id),
 activity_revision bigint NOT NULL,
 response text NOT NULL CHECK(response IN ('attending','declined')),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(activity_id,actor_id)
);
