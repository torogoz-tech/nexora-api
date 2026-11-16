ALTER TABLE actor_roles DROP CONSTRAINT actor_roles_role_check;
ALTER TABLE actor_roles ADD CONSTRAINT actor_roles_role_check CHECK(role IN ('user','supervisor','developer','technical_lead','pmo','manager','super_admin'));
CREATE FUNCTION isolate_request_members() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.role IN ('user','supervisor') THEN
  IF NEW.all_projects THEN RAISE EXCEPTION 'request members cannot access IT projects'; END IF;
  IF EXISTS(SELECT 1 FROM tasks WHERE assignee_id=NEW.actor_id AND NOT archived AND lifecycle NOT IN ('completed','pending')) THEN RAISE EXCEPTION 'reassign active IT tasks before changing workspace'; END IF;
  DELETE FROM actor_grants WHERE actor_id=NEW.actor_id;
  INSERT INTO actor_grants(actor_id,capability,project_id) VALUES(NEW.actor_id,'requests:read',NULL),(NEW.actor_id,'requests:write',NULL);
  DELETE FROM member_role_projects WHERE actor_id=NEW.actor_id;
  DELETE FROM developer_workflows WHERE actor_id=NEW.actor_id;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER isolate_request_members AFTER INSERT OR UPDATE ON actor_roles FOR EACH ROW EXECUTE FUNCTION isolate_request_members();
CREATE TABLE service_request_types (
 id text PRIMARY KEY, name text NOT NULL CHECK(length(btrim(name)) BETWEEN 1 AND 120),
 archived boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0)
);
CREATE UNIQUE INDEX service_request_type_name ON service_request_types(lower(btrim(name)));
INSERT INTO service_request_types(id,name) VALUES('support','Soporte'),('incident','Incidente'),('service','Servicio');
CREATE TABLE service_requests (
 id text PRIMARY KEY, number bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
 requester_id text NOT NULL REFERENCES actors(id), type_id text NOT NULL REFERENCES service_request_types(id),
 title text NOT NULL CHECK(length(btrim(title)) BETWEEN 1 AND 300), body text NOT NULL,
 status text NOT NULL DEFAULT 'new' CHECK(status IN ('new','in_progress','completed')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX service_request_owner ON service_requests(requester_id,created_at DESC,id);
CREATE TABLE service_request_comments (
 id text PRIMARY KEY, request_id text NOT NULL REFERENCES service_requests(id), actor_id text NOT NULL REFERENCES actors(id),
 body text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE service_request_history (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, request_id text NOT NULL REFERENCES service_requests(id),
 actor_id text NOT NULL REFERENCES actors(id), action text NOT NULL, detail jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE service_request_links (
 request_id text NOT NULL REFERENCES service_requests(id), kind text NOT NULL CHECK(kind IN ('task','feature')),
 target_id text NOT NULL, project_id text REFERENCES projects(id), linked_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY(request_id,kind,target_id)
);
CREATE TABLE service_request_attachments (
 id text PRIMARY KEY, request_id text NOT NULL REFERENCES service_requests(id), actor_id text NOT NULL REFERENCES actors(id),
 name text NOT NULL, media_type text NOT NULL, content bytea NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

INSERT INTO actor_grants(actor_id,capability,project_id) SELECT r.actor_id,c.capability,NULL FROM actor_roles r CROSS JOIN (VALUES('requests:read'),('requests:write')) c(capability) ON CONFLICT DO NOTHING;
