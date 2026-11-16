-- Existing identities must never silently change their credited member.
SELECT pg_advisory_xact_lock(hashtextextended(current_schema()||':git-author-aliases',0));
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM git_author_aliases WHERE revoked_at IS NULL
            GROUP BY provider,host,email HAVING count(DISTINCT actor_id)>1) THEN
  RAISE EXCEPTION 'Resolve conflicting Git author members before organization-wide migration';
 END IF;
END $$;
-- Keep redundant rows for audit and already-attributed commits.
WITH ranked AS (
 SELECT id,row_number() OVER (PARTITION BY provider,host,email ORDER BY created_at,id) AS ordinal
 FROM git_author_aliases WHERE revoked_at IS NULL
)
UPDATE git_author_aliases a SET revoked_at=clock_timestamp(),revoked_by=a.created_by,
 revocation_reason='Consolidated organization-wide Git identity',revision=a.revision+1
FROM ranked r WHERE a.id=r.id AND r.ordinal>1;
UPDATE git_author_aliases SET project_id=NULL,revision=revision+1
 WHERE revoked_at IS NULL AND project_id IS NOT NULL;
ALTER TABLE git_author_aliases ADD CONSTRAINT git_author_alias_organization_scope
 CHECK (revoked_at IS NOT NULL OR project_id IS NULL);
CREATE UNIQUE INDEX git_author_alias_organization_identity
 ON git_author_aliases(provider,host,email) WHERE revoked_at IS NULL;
-- Recover previously imported work without re-downloading or re-analyzing code.
WITH attributed AS (
 UPDATE git_commit_authorship c SET actor_id=a.actor_id,alias_id=a.id,attributed_at=clock_timestamp()
 FROM git_author_aliases a,actors member
 WHERE c.actor_id IS NULL AND c.author_email=a.email AND c.provider=a.provider AND c.host=a.host
 AND a.revoked_at IS NULL AND member.id=a.actor_id AND member.active AND member.kind='human'
 RETURNING c.actor_id
)
INSERT INTO developer_rating_dirty(actor_id) SELECT DISTINCT actor_id FROM attributed ON CONFLICT DO NOTHING;
