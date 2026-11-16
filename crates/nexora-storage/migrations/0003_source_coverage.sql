BEGIN;
-- Explicit analyzer coverage is part of the snapshot facts, not policy findings.
CREATE TABLE source_analysis_coverage (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    file_id text NOT NULL,
    relation text NOT NULL CHECK (relation IN ('DECLARES','IMPORTS','DEPENDS_ON')),
    domain text NOT NULL,
    complete boolean NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, file_id, relation, domain),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, file_id) REFERENCES source_files ON DELETE CASCADE
);
COMMIT;
