ALTER TABLE tasks ADD COLUMN acceptance_criteria jsonb NOT NULL DEFAULT '[]'::jsonb
 CHECK(jsonb_typeof(acceptance_criteria)='array');
