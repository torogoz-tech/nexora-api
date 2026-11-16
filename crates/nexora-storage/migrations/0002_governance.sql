BEGIN;
-- Policy catalogs are optional. YAML may remain the policy source of truth.
CREATE TABLE governance_policies (
    tenant_id text NOT NULL,
    policy_id text NOT NULL,
    name text NOT NULL,
    category text NOT NULL,
    PRIMARY KEY (tenant_id, policy_id)
);
CREATE TABLE governance_policy_versions (
    tenant_id text NOT NULL,
    policy_id text NOT NULL,
    policy_version text NOT NULL,
    content_hash text NOT NULL,
    source_uri text,
    policy_document jsonb NOT NULL,
    PRIMARY KEY (tenant_id, policy_id, policy_version),
    FOREIGN KEY (tenant_id, policy_id) REFERENCES governance_policies ON DELETE CASCADE
);
CREATE TABLE governance_evaluations (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    evaluation_id text NOT NULL,
    engine_version text NOT NULL,
    policy_set_hash text NOT NULL,
    policy_manifest jsonb NOT NULL,
    coverage jsonb NOT NULL,
    result text NOT NULL CHECK (result IN ('PASS','FAIL','UNKNOWN')),
    created_at timestamptz NOT NULL DEFAULT now(),
    report jsonb NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, evaluation_id),
    UNIQUE (tenant_id, repository_id, evaluation_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id) REFERENCES repository_snapshots ON DELETE CASCADE
);
CREATE TABLE governance_findings (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    evaluation_id text NOT NULL,
    finding_id text NOT NULL,
    policy_id text NOT NULL,
    policy_version text NOT NULL,
    node_id text,
    severity text NOT NULL CHECK (severity IN ('info','low','medium','high','critical')),
    result text NOT NULL CHECK (result IN ('PASS','FAIL','UNKNOWN')),
    analysis_diagnostics jsonb NOT NULL DEFAULT '[]',
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, evaluation_id, finding_id),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, evaluation_id) REFERENCES governance_evaluations ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, node_id) REFERENCES code_nodes
);
CREATE TABLE governance_evidence (
    tenant_id text NOT NULL,
    repository_id text NOT NULL,
    snapshot_id text NOT NULL,
    evaluation_id text NOT NULL,
    finding_id text NOT NULL,
    evidence_sequence integer NOT NULL CHECK (evidence_sequence >= 0),
    file_id text,
    predicate text NOT NULL,
    result text NOT NULL CHECK (result IN ('PASS','FAIL','UNKNOWN')),
    polarity text NOT NULL CHECK (polarity IN ('positive','negative','insufficient')),
    start_byte bigint CHECK (start_byte >= 0),
    end_byte bigint CHECK (end_byte >= start_byte),
    inspected_scope text NOT NULL,
    exhaustive boolean NOT NULL,
    details jsonb NOT NULL,
    PRIMARY KEY (tenant_id, repository_id, snapshot_id, evaluation_id, finding_id, evidence_sequence),
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, evaluation_id, finding_id) REFERENCES governance_findings ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, repository_id, snapshot_id, file_id) REFERENCES source_files
);
CREATE INDEX governance_evaluations_by_snapshot ON governance_evaluations (tenant_id, repository_id, snapshot_id, created_at DESC);
CREATE INDEX governance_findings_by_policy ON governance_findings (tenant_id, repository_id, snapshot_id, policy_id, policy_version, result);
CREATE INDEX governance_evidence_by_file ON governance_evidence (tenant_id, repository_id, snapshot_id, file_id);
-- Findings reference policy/version text even without an optional SQL catalog entry.
-- The immutable policy_set_hash and manifest identify file-based policy versions.
COMMIT;
