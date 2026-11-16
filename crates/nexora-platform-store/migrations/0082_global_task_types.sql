-- Task types are installation-wide. Keep task_catalog_type as the authoritative reference.
ALTER TABLE tasks DROP CONSTRAINT tasks_project_id_type_code_fkey;
