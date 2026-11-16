CREATE TABLE actor_roles(
 actor_id text PRIMARY KEY REFERENCES actors(id),
 role text NOT NULL CHECK(role IN ('developer','pmo','manager','super_admin')),
 all_projects boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 CHECK(role<>'super_admin' OR all_projects)
);
CREATE TABLE member_role_projects(
 actor_id text NOT NULL REFERENCES actor_roles(actor_id) ON DELETE CASCADE,
 project_id text NOT NULL REFERENCES projects(id),
 PRIMARY KEY(actor_id,project_id)
);
-- Preserve existing installation authority; ordinary legacy grants remain unchanged.
INSERT INTO actor_roles(actor_id,role,all_projects)
 SELECT DISTINCT a.id,'super_admin',true FROM actors a JOIN actor_grants g ON g.actor_id=a.id
 WHERE a.kind='human' AND g.capability='installation:manage' AND g.project_id IS NULL;
