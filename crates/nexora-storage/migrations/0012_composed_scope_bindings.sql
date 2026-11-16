-- Prepared only. No PostgreSQL adapter or migration execution is performed.
BEGIN;
ALTER TABLE code_edges DROP CONSTRAINT code_edges_kind_check;
ALTER TABLE code_edges ADD CONSTRAINT code_edges_kind_check CHECK
    (kind IN ('CONTAINS','DECLARES','IMPORTS','EXPORTS','CALLS','REFERENCES','EXTENDS','IMPLEMENTS','DEPENDS_ON','INTERPRETS','SCOPE_BRIDGE'));
ALTER TABLE code_edges ADD COLUMN scope_binding jsonb;
COMMIT;
