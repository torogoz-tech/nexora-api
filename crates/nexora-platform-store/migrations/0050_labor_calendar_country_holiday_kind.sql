ALTER TABLE labor_holidays ADD COLUMN kind text NOT NULL DEFAULT 'holiday' CHECK(kind IN ('holiday','public_holiday'));
UPDATE labor_calendar_settings SET country=(SELECT NULLIF(country_code,'') FROM organization_profile WHERE singleton),revision=revision+1 WHERE singleton;
CREATE FUNCTION sync_organization_labor_country() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.country_code IS DISTINCT FROM OLD.country_code THEN
  UPDATE labor_calendar_settings SET country=NULLIF(NEW.country_code,''),revision=revision+1 WHERE singleton;
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER organization_labor_country AFTER UPDATE OF country_code ON organization_profile FOR EACH ROW EXECUTE FUNCTION sync_organization_labor_country();
