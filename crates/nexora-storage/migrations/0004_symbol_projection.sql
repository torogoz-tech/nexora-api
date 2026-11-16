-- Prepared schema evolution; no PostgreSQL adapter/migrator is executed by Nexora yet.
ALTER TABLE code_nodes DROP CONSTRAINT code_nodes_kind_check;
ALTER TABLE code_nodes ADD CONSTRAINT code_nodes_kind_check CHECK
    (kind IN ('Repository','File','Module','Namespace','Class','Interface','Function','Method','Variable','Parameter','Symbol'));
ALTER TABLE code_edges DROP CONSTRAINT code_edges_kind_check;
ALTER TABLE code_edges ADD CONSTRAINT code_edges_kind_check CHECK
    (kind IN ('CONTAINS','DECLARES','IMPORTS','EXPORTS','CALLS','REFERENCES','EXTENDS','IMPLEMENTS','DEPENDS_ON'));
ALTER TABLE code_edges ADD COLUMN label text CHECK
    (label IS NULL OR (octet_length(label) BETWEEN 1 AND 128 AND label !~ '[[:cntrl:]]'));
