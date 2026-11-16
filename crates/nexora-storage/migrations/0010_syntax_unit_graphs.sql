-- Prepared only. No database adapter or migrator is executed by this change.
BEGIN;
ALTER TABLE code_nodes DROP CONSTRAINT code_nodes_kind_check;
ALTER TABLE code_nodes ADD CONSTRAINT code_nodes_kind_check CHECK
    (kind IN ('Repository','File','Module','Namespace','Class','Interface','Function','Method','Variable','Parameter','Symbol','Block','Expression','Statement','TypeAlias','Enum','LexicalRegion','SyntaxUnit'));
ALTER TABLE code_edges DROP CONSTRAINT code_edges_kind_check;
ALTER TABLE code_edges ADD CONSTRAINT code_edges_kind_check CHECK
    (kind IN ('CONTAINS','DECLARES','IMPORTS','EXPORTS','CALLS','REFERENCES','EXTENDS','IMPLEMENTS','DEPENDS_ON','INTERPRETS'));

CREATE TABLE syntax_units (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    unit_node_id text NOT NULL,
    provenance jsonb NOT NULL,
    syntax_coverage jsonb NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, unit_node_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, unit_node_id)
        REFERENCES code_nodes (tenant_id, repository_id, snapshot_id, node_id)
        ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
ALTER TABLE code_nodes ADD COLUMN syntax_unit_id text;
ALTER TABLE code_nodes ADD CONSTRAINT code_nodes_syntax_unit_fk
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, syntax_unit_id)
    REFERENCES syntax_units DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE analysis_evidence ADD COLUMN syntax_unit_id text;
ALTER TABLE analysis_evidence ADD CONSTRAINT evidence_syntax_unit_fk
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, syntax_unit_id)
    REFERENCES syntax_units DEFERRABLE INITIALLY DEFERRED;
COMMIT;
