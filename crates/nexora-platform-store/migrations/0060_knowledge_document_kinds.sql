-- Additive document kinds. Existing identities, versions and checksums are unchanged.
ALTER TABLE project_documents DROP CONSTRAINT project_documents_kind_check;
ALTER TABLE project_documents ADD CONSTRAINT project_documents_kind_check
 CHECK(kind IN ('requirement','business_rule','decision','document','best_practice','technical_debt'));
