CREATE TABLE organization_profile (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 name text NOT NULL DEFAULT '', address text NOT NULL DEFAULT '', phone text NOT NULL DEFAULT '', country_code text NOT NULL DEFAULT '',
 logo bytea, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 CHECK(octet_length(name)<=160 AND octet_length(address)<=2000 AND octet_length(phone)<=64),
 CHECK(country_code='' OR country_code ~ '^[A-Z]{2}$'), CHECK(logo IS NULL OR octet_length(logo)<=2097152)
);
INSERT INTO organization_profile(singleton,name) VALUES(true,COALESCE((SELECT organization_name FROM installation WHERE singleton),''));
CREATE TABLE installation_services (
 kind text PRIMARY KEY CHECK(kind IN ('openai','smtp')), settings jsonb NOT NULL CHECK(jsonb_typeof(settings)='object'),
 secret jsonb, secret_revision bigint NOT NULL DEFAULT 1 CHECK(secret_revision>0),
 managed boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0)
);
INSERT INTO installation_services(kind,settings) VALUES
 ('openai','{"endpoint":"https://api.openai.com/v1","model":"","effort":"medium","enabled":false}'),
 ('smtp','{"enabled":false,"host":"","port":587,"username":"","sender_email":"","sender_name":"Nexora","tls_mode":"starttls"}');
CREATE FUNCTION signal_branding_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN IF NEW.revision<>OLD.revision THEN UPDATE installation_preferences SET revision=revision+1 WHERE singleton; END IF; RETURN NEW; END $$;
CREATE TRIGGER organization_live_update AFTER UPDATE ON organization_profile FOR EACH ROW EXECUTE FUNCTION signal_branding_update();
