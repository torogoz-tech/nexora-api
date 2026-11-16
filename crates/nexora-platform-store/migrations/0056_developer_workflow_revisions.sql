-- Keep the last issued membership revision after an assignment is removed.
CREATE TABLE developer_workflow_revisions (
    actor_id text PRIMARY KEY REFERENCES actors(id),
    revision bigint NOT NULL CHECK (revision > 0)
);
INSERT INTO developer_workflow_revisions(actor_id,revision)
SELECT actor_id,revision FROM developer_workflows;
-- Migration30 could seed one initial membership per actor without an
-- assignment audit event. Reserve that possible revision for EVERY existing
-- actor, including those with neither a current row nor assignment events.
-- Subsequent application assignments are audited; count + 1 therefore bounds
-- all previously issued revisions, even across deleted generations.
INSERT INTO developer_workflow_revisions(actor_id,revision)
SELECT a.id,count(e.resource_id)+1 FROM actors a
LEFT JOIN audit_events e ON e.resource_id=a.id
    AND e.operation='developer.workflow_assigned'
GROUP BY a.id
ON CONFLICT(actor_id) DO UPDATE
SET revision=GREATEST(developer_workflow_revisions.revision,excluded.revision);
