ALTER TABLE workflows DROP CONSTRAINT workflows_purpose_check;
ALTER TABLE workflows ADD CONSTRAINT workflows_purpose_check CHECK (octet_length(purpose) <= 4000);
ALTER TABLE workflow_versions DROP CONSTRAINT workflow_versions_purpose_check;
ALTER TABLE workflow_versions ADD CONSTRAINT workflow_versions_purpose_check CHECK (octet_length(purpose) <= 4000);
