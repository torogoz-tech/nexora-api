-- Prepared only: PostgreSQL adapter and migration execution remain outside this milestone.
ALTER TABLE code_nodes DROP CONSTRAINT code_nodes_kind_check;
ALTER TABLE code_nodes ADD CONSTRAINT code_nodes_kind_check CHECK
    (kind IN ('Repository','File','Module','Namespace','Class','Interface','Function','Method','Variable','Parameter','Symbol','Block','Expression','Statement'));
-- Shape: expression kind, optional operator and ordered, file-qualified operand IDs.
ALTER TABLE code_nodes ADD COLUMN expression jsonb;
