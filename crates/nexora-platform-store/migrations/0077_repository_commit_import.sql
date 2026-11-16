CREATE TABLE repository_commit_imports (
 repository_id text PRIMARY KEY REFERENCES project_repositories(id),
 target_commit text NOT NULL,
 baseline_commit text,
 mode text NOT NULL CHECK(mode IN ('full','compare')),
 next_page bigint NOT NULL DEFAULT 1 CHECK(next_page>0),
 complete boolean NOT NULL DEFAULT false,
 imported_count bigint NOT NULL DEFAULT 0,
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
