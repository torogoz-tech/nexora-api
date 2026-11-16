-- Legacy codes and knowledge associations remain intact; purposes are explicit, never inferred from labels.
ALTER TABLE task_type_catalog ADD COLUMN purpose text NOT NULL DEFAULT 'general'
 CHECK(purpose IN ('product_design','implementation','general'));
ALTER TABLE tasks ADD COLUMN entry_workflow_id text REFERENCES workflows(id);
ALTER TABLE tasks ADD COLUMN primary_requirement_id text REFERENCES project_documents(id);
ALTER TABLE tasks ADD COLUMN primary_feature_id text REFERENCES project_features(id);
ALTER TABLE tasks ADD CONSTRAINT tasks_one_primary_knowledge CHECK(primary_requirement_id IS NULL OR primary_feature_id IS NULL);
ALTER TABLE tasks ADD COLUMN purpose text NOT NULL DEFAULT 'general' CHECK(purpose IN ('product_design','implementation','general'));
ALTER TABLE tasks ADD COLUMN governed_stage boolean NOT NULL DEFAULT false;
ALTER TABLE tasks ALTER COLUMN governed_stage SET DEFAULT true;
