-- Prepared only; database adapter and execution remain pending.
ALTER TABLE code_nodes DROP CONSTRAINT code_nodes_kind_check;
ALTER TABLE code_nodes ADD CONSTRAINT code_nodes_kind_check CHECK
    (kind IN ('Repository','File','Module','Namespace','Class','Interface','Function','Method','Variable','Parameter','Symbol','Block','Expression','Statement','TypeAlias','Enum','LexicalRegion'));
ALTER TABLE code_nodes ADD COLUMN lexical_region jsonb;
