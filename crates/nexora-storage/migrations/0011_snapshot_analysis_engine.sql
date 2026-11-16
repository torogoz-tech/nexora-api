-- Prepared only; PostgreSQL persistence/migration execution remains pending.
-- Snapshot.analysis_configuration corresponds to the existing rules_hash column.
ALTER TABLE repository_snapshots ADD COLUMN analysis_engine text;
