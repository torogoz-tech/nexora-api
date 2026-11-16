ALTER TABLE actor_roles ADD COLUMN rating_started_at timestamptz NOT NULL DEFAULT clock_timestamp();
CREATE TABLE developer_push_activity(
 actor_id text NOT NULL REFERENCES actors(id),
 provider text NOT NULL CHECK(provider IN ('github','gitlab')),
 scope_id text NOT NULL,delivery_id text NOT NULL,
 project_id text NOT NULL REFERENCES projects(id),
 occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(actor_id,provider,scope_id,delivery_id,project_id)
);
CREATE TABLE developer_rating_entries(
 id text PRIMARY KEY,
 actor_id text NOT NULL REFERENCES actors(id),
 date date NOT NULL,
 source text NOT NULL CHECK(source IN ('activity','peer','manager','ai')),
 score double precision NOT NULL CHECK(score>=0 AND score<=5),
 reason text NOT NULL,
 evidence_url text,
 created_by text REFERENCES actors(id),
 project_id text REFERENCES projects(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK((source IN ('activity','ai') AND created_by IS NULL) OR (source IN ('peer','manager') AND created_by IS NOT NULL AND created_by<>actor_id))
);
CREATE UNIQUE INDEX developer_rating_activity_day ON developer_rating_entries(actor_id,date) WHERE source='activity';
CREATE INDEX developer_rating_actor_date ON developer_rating_entries(actor_id,date);
CREATE INDEX developer_rating_activity_date ON developer_rating_entries(date,actor_id) WHERE source='activity';
CREATE FUNCTION developer_rating_excluded(member text,day date) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT(extract(isodow FROM day)::integer=ANY(s.workdays))
 OR EXISTS(SELECT 1 FROM labor_holidays h WHERE h.date=day AND h.status='approved' AND (h.country IS NULL OR h.country=s.country))
 OR EXISTS(SELECT 1 FROM labor_leave_requests l WHERE l.actor_id=member AND l.status='approved' AND day BETWEEN l.start_date AND l.end_date AND (l.minutes_per_day IS NULL OR l.minutes_per_day>=s.daily_minutes))
 FROM labor_calendar_settings s WHERE singleton
$$;
CREATE FUNCTION developer_rating_enrollment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.role='developer' AND OLD.role<>'developer' THEN NEW.rating_started_at=clock_timestamp(); END IF;
 RETURN NEW;
END
$$;
CREATE TRIGGER developer_rating_enrollment BEFORE UPDATE OF role ON actor_roles FOR EACH ROW EXECUTE FUNCTION developer_rating_enrollment();
ALTER TABLE developer_push_activity ADD COLUMN processed boolean NOT NULL DEFAULT false;
CREATE INDEX developer_push_actor_time ON developer_push_activity(actor_id,occurred_at);
CREATE INDEX developer_push_pending ON developer_push_activity(actor_id,occurred_at) WHERE NOT processed;
CREATE TABLE developer_rating_closures (
 actor_id text PRIMARY KEY REFERENCES actors(id),last_closed date NOT NULL,calendar_revision bigint NOT NULL
);
CREATE TABLE developer_rating_dirty (actor_id text PRIMARY KEY REFERENCES actors(id));
CREATE FUNCTION developer_rating_calendar_dirty() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='labor_leave_requests' THEN
  IF TG_OP<>'INSERT' THEN INSERT INTO developer_rating_dirty(actor_id) VALUES(OLD.actor_id) ON CONFLICT DO NOTHING; END IF;
  IF TG_OP<>'DELETE' THEN INSERT INTO developer_rating_dirty(actor_id) VALUES(NEW.actor_id) ON CONFLICT DO NOTHING; END IF;
 ELSE
  INSERT INTO developer_rating_dirty(actor_id) SELECT actor_id FROM actor_roles WHERE role='developer' ON CONFLICT DO NOTHING;
 END IF;
 RETURN NULL;
END
$$;
CREATE TRIGGER developer_rating_leave_dirty AFTER INSERT OR UPDATE OR DELETE ON labor_leave_requests FOR EACH ROW EXECUTE FUNCTION developer_rating_calendar_dirty();
CREATE TRIGGER developer_rating_holiday_dirty AFTER INSERT OR UPDATE OR DELETE ON labor_holidays FOR EACH STATEMENT EXECUTE FUNCTION developer_rating_calendar_dirty();
CREATE TRIGGER developer_rating_settings_dirty AFTER UPDATE ON labor_calendar_settings FOR EACH STATEMENT EXECUTE FUNCTION developer_rating_calendar_dirty();
