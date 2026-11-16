CREATE TABLE installation_navigation (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 paths jsonb NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(paths)='array' AND jsonb_array_length(paths)<=64),
 revision bigint NOT NULL DEFAULT 0 CHECK(revision>=0)
);
INSERT INTO installation_navigation(singleton) VALUES(true);
