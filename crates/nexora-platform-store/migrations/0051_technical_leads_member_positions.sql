ALTER TABLE actor_roles DROP CONSTRAINT actor_roles_role_check;
ALTER TABLE actor_roles ADD CONSTRAINT actor_roles_role_check CHECK(role IN ('developer','technical_lead','pmo','manager','super_admin'));
CREATE TABLE member_positions(actor_id text PRIMARY KEY REFERENCES actors(id),position text NOT NULL DEFAULT '',revision bigint NOT NULL DEFAULT 1 CHECK(revision>0));
CREATE OR REPLACE FUNCTION developer_rating_enrollment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.role IN ('developer','technical_lead') AND OLD.role NOT IN ('developer','technical_lead') THEN NEW.rating_started_at=clock_timestamp(); END IF;
 RETURN NEW;
END
$$;
CREATE OR REPLACE FUNCTION developer_rating_calendar_dirty() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='labor_leave_requests' THEN
  IF TG_OP<>'INSERT' THEN INSERT INTO developer_rating_dirty(actor_id) VALUES(OLD.actor_id) ON CONFLICT DO NOTHING; END IF;
  IF TG_OP<>'DELETE' THEN INSERT INTO developer_rating_dirty(actor_id) VALUES(NEW.actor_id) ON CONFLICT DO NOTHING; END IF;
 ELSE
  INSERT INTO developer_rating_dirty(actor_id) SELECT actor_id FROM actor_roles WHERE role IN ('developer','technical_lead') ON CONFLICT DO NOTHING;
 END IF;
 RETURN NULL;
END
$$;
