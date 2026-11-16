BEGIN;

CREATE TABLE repositories (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    name text NOT NULL,
    root_uri text NOT NULL,
    current_snapshot_id text,
    PRIMARY KEY (tenant_id, repository_id)
);

CREATE TABLE repository_snapshots (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    rules_version text NOT NULL,
    rules_hash text NOT NULL,
    status text NOT NULL CHECK (status IN ('staging', 'published', 'failed')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, repository_id, snapshot_id),
    FOREIGN KEY (tenant_id, repository_id) REFERENCES repositories ON DELETE CASCADE
);
ALTER TABLE repositories ADD CONSTRAINT repositories_current_snapshot_fk
    FOREIGN KEY (tenant_id, repository_id, current_snapshot_id)
    REFERENCES repository_snapshots (tenant_id, repository_id, snapshot_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE source_files (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    file_id text NOT NULL,
    relative_path text NOT NULL CHECK (relative_path <> '' AND relative_path !~ '(^/|(^|/)\.\.(/|$))'),
    language_id text NOT NULL,
    content_hash text NOT NULL,
    content text NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, file_id),
    UNIQUE (tenant_id, repository_id, snapshot_id, relative_path),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id) REFERENCES repository_snapshots ON DELETE CASCADE
);

CREATE TABLE analysis_evidence (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    evidence_id text NOT NULL,
    file_id text NOT NULL,
    start_byte bigint NOT NULL CHECK (start_byte >= 0),
    end_byte bigint NOT NULL CHECK (end_byte >= start_byte),
    cst_id text,
    ast_id text,
    rule_version text NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, evidence_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, file_id) REFERENCES source_files ON DELETE CASCADE
);

CREATE TABLE code_nodes (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    node_id text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('Repository','File','Module','Namespace','Class','Interface','Function','Method','Variable','Parameter')),
    name text NOT NULL,
    evidence_id text,
    properties jsonb NOT NULL DEFAULT '{}',
    search_document tsvector GENERATED ALWAYS AS (to_tsvector('simple', name)) STORED,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, node_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id) REFERENCES repository_snapshots ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, evidence_id) REFERENCES analysis_evidence
);
CREATE INDEX code_nodes_name ON code_nodes (tenant_id, repository_id, snapshot_id, name);
CREATE INDEX code_nodes_search ON code_nodes USING gin (search_document);

CREATE TABLE code_edges (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    edge_id text NOT NULL,
    source_node_id text NOT NULL,
    target_node_id text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('CONTAINS','DECLARES','IMPORTS','CALLS','REFERENCES','EXTENDS','IMPLEMENTS','DEPENDS_ON')),
    resolution text NOT NULL CHECK (resolution IN ('Resolved','Ambiguous','Unresolved')),
    evidence_id text NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, edge_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, source_node_id) REFERENCES code_nodes ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, target_node_id) REFERENCES code_nodes ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, evidence_id) REFERENCES analysis_evidence
);
CREATE INDEX code_edges_outgoing ON code_edges (tenant_id, repository_id, snapshot_id, source_node_id, kind);
CREATE INDEX code_edges_incoming ON code_edges (tenant_id, repository_id, snapshot_id, target_node_id, kind);

CREATE TABLE source_chunks (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    chunk_id text NOT NULL,
    file_id text NOT NULL,
    start_byte bigint NOT NULL CHECK (start_byte >= 0),
    end_byte bigint NOT NULL CHECK (end_byte >= start_byte),
    content text NOT NULL,
    search_document tsvector GENERATED ALWAYS AS (to_tsvector('simple', content)) STORED,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, chunk_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, file_id) REFERENCES source_files ON DELETE CASCADE
);
CREATE INDEX source_chunks_search ON source_chunks USING gin (search_document);

CREATE TABLE analysis_diagnostics (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    diagnostic_id text NOT NULL,
    file_id text,
    stage text NOT NULL,
    message text NOT NULL,
    start_byte bigint CHECK (start_byte >= 0),
    end_byte bigint CHECK (end_byte >= start_byte),
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, diagnostic_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id) REFERENCES repository_snapshots ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, file_id) REFERENCES source_files
);
CREATE INDEX diagnostics_by_snapshot ON analysis_diagnostics (tenant_id, repository_id, snapshot_id, stage);

-- Future: a separate embedding table keyed by tenant/repository/snapshot/chunk/model.
-- No pgvector dependency, dimensions or generated embeddings in bootstrap.
COMMIT;
