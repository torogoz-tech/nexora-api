-- Prepared only: PostgreSQL storage/migration execution remains outside the current MVP.
ALTER TABLE code_nodes DROP CONSTRAINT code_nodes_kind_check;
ALTER TABLE code_nodes ADD CONSTRAINT code_nodes_kind_check CHECK
    (kind IN ('Repository','File','Module','Namespace','Class','Interface','Function','Method','Variable','Parameter','Symbol','Block'));
ALTER TABLE code_nodes ADD COLUMN scope_id text;
ALTER TABLE code_nodes ADD COLUMN owned_scope_id text;
ALTER TABLE code_nodes ADD COLUMN scope_policy jsonb;
ALTER TABLE code_nodes ADD COLUMN syntax_regions jsonb NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE code_nodes ADD COLUMN syntax_captures jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE repository_snapshots ADD COLUMN syntax_coverage jsonb NOT NULL DEFAULT '{}'::jsonb;
