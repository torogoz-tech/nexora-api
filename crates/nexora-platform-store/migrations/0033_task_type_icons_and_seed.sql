ALTER TABLE task_type_catalog ADD COLUMN icon text;
-- No user-selected icons existed before this migration. Preserve all existing
-- names, descriptions, colors, archived flags and project choices.
UPDATE task_type_catalog SET icon=CASE
 WHEN code='bug' THEN 'bug'
 WHEN code IN ('feature','improvement') THEN 'lightbulb'
 WHEN code='research' THEN 'search'
 WHEN code IN ('chore','maintenance') THEN 'wrench'
 WHEN code='incident' THEN 'shield-alert'
 ELSE 'list-todo' END;
ALTER TABLE task_type_catalog ALTER COLUMN icon SET DEFAULT 'list-todo';
ALTER TABLE task_type_catalog ALTER COLUMN icon SET NOT NULL;
ALTER TABLE task_type_catalog ADD CONSTRAINT task_type_icon_allowlist CHECK(icon IN (
 'list-todo','bug','lightbulb','search','wrench','shield-alert',
 'git-pull-request','file-check','message-square','flask-conical','code','flag'
));
INSERT INTO task_type_catalog(code,name,description,color,icon) VALUES
 ('task','Tarea','Trabajo concreto que debe realizarse.',NULL,'list-todo'),
 ('bug','Error','Comportamiento incorrecto que necesita una corrección.','#EF4444','bug'),
 ('improvement','Mejora','Ajuste o ampliación de una funcionalidad existente.','#A855F7','lightbulb'),
 ('research','Investigación','Exploración técnica para resolver una pregunta o reducir incertidumbre.','#3B82F6','search'),
 ('maintenance','Mantenimiento','Actualización, limpieza o mantenimiento técnico.','#F59E0B','wrench'),
 ('incident','Incidente','Interrupción o problema operativo que requiere atención.','#EF4444','shield-alert')
ON CONFLICT(code) DO NOTHING;
-- A shared seed never enrolls projects automatically.
