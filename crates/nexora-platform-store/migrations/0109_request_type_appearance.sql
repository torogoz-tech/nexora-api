ALTER TABLE service_request_types ADD COLUMN icon text NOT NULL DEFAULT 'headset', ADD COLUMN color text NOT NULL DEFAULT '#38BDF8' CHECK(color ~ '^#[0-9A-Fa-f]{6}$');
UPDATE service_request_types SET icon='shield-check',color='#F59E0B' WHERE id='incident';
UPDATE service_request_types SET icon='briefcase',color='#A78BFA' WHERE id='service';
