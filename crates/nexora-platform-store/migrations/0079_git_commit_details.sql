ALTER TABLE git_commit_authorship ADD COLUMN commit_message text;
CREATE TABLE repository_branch_imports (
 repository_id text NOT NULL REFERENCES project_repositories(id),
 branch_name text NOT NULL, target_commit text NOT NULL,
 next_page bigint NOT NULL DEFAULT 1 CHECK(next_page>0),
 complete boolean NOT NULL DEFAULT false,
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(repository_id,branch_name)
);
CREATE TABLE git_commit_branches (
 repository_id text NOT NULL REFERENCES project_repositories(id),
 commit_sha text NOT NULL, branch_name text NOT NULL,
 PRIMARY KEY(repository_id,commit_sha,branch_name)
);
