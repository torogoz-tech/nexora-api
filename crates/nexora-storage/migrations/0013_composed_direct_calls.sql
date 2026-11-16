-- Prepared only. PostgreSQL adapter and migration execution remain pending.
BEGIN;
ALTER TABLE code_edges ADD COLUMN call_binding jsonb;
ALTER TABLE code_nodes ADD COLUMN mutation_operand text;
ALTER TABLE code_nodes ADD COLUMN expression_policy_id text;
ALTER TABLE repository_snapshots ADD COLUMN expression_policies jsonb NOT NULL DEFAULT '{}'::jsonb;
COMMIT;
