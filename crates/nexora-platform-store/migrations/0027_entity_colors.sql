-- Additive display metadata. Historical workflow JSON stays untouched.
ALTER TABLE projects ADD COLUMN color text CHECK (color ~ '^#[0-9A-F]{6}$');
ALTER TABLE backlogs ADD COLUMN color text CHECK (color ~ '^#[0-9A-F]{6}$');
ALTER TABLE task_types ADD COLUMN color text CHECK (color ~ '^#[0-9A-F]{6}$');
