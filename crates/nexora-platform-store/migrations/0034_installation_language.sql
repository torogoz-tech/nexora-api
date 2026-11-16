CREATE TABLE installation_preferences (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 language text NOT NULL DEFAULT 'es' CHECK(language IN ('es','en')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0)
);
INSERT INTO installation_preferences(singleton) VALUES(true);
CREATE TRIGGER installation_preferences_live_update AFTER UPDATE ON installation_preferences FOR EACH ROW EXECUTE FUNCTION signal_project_live_update('preferences','global');
