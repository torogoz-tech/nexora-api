ALTER TABLE workflows ADD COLUMN color text CHECK(color IS NULL OR color ~ '^#[A-Fa-f0-9]{6}$');
