ALTER TABLE service_request_types ADD COLUMN sla jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE service_requests ADD COLUMN criticality text NOT NULL DEFAULT 'medium' CHECK(criticality IN ('low','medium','high','critical')),
 ADD COLUMN sla_snapshot jsonb, ADD COLUMN first_attention_at timestamptz, ADD COLUMN resolved_at timestamptz;
-- Existing records keep no SLA; derive service timestamps only from recorded history.
UPDATE service_requests r SET first_attention_at=(SELECT min(created_at) FROM service_request_history h WHERE h.request_id=r.id AND h.action='changed' AND h.detail->>'status' IN ('in_progress','completed')),
 resolved_at=CASE WHEN status='completed' THEN updated_at END;
CREATE FUNCTION request_sla_due(start_at timestamptz, minutes integer, calendar jsonb, continuous boolean) RETURNS timestamptz LANGUAGE plpgsql STABLE AS $$
DECLARE day date; zone text; a timestamptz; b timestamptz; remaining double precision:=minutes*60; available double precision;
BEGIN
 IF continuous THEN RETURN start_at+minutes*interval '1 minute'; END IF;
 zone:=calendar->>'timezone';day:=(start_at AT TIME ZONE zone)::date-1;
 LOOP
  IF extract(isodow FROM day)::integer IN (SELECT jsonb_array_elements_text(calendar->'workdays')::integer)
   AND NOT EXISTS(SELECT 1 FROM labor_holidays h WHERE h.date=day AND h.status='approved' AND (h.country IS NULL OR h.country=calendar->>'country')) THEN
   a:=(day+(calendar->>'work_start')::time) AT TIME ZONE zone;
   b:=((day+CASE WHEN calendar->>'work_end'<=calendar->>'work_start' THEN 1 ELSE 0 END)+(calendar->>'work_end')::time) AT TIME ZONE zone;
   a:=greatest(a,start_at);available:=greatest(0,extract(epoch FROM b-a));
   IF available>=remaining THEN RETURN a+remaining*interval '1 second';END IF;
   remaining:=remaining-available;
  END IF;
  day:=day+1;
 END LOOP;
END $$;
CREATE FUNCTION request_sla_snapshot(type_id text, level text, started timestamptz) RETURNS jsonb LANGUAGE plpgsql STABLE AS $$
DECLARE policy jsonb; calendar jsonb;
BEGIN
 SELECT t.sla->level INTO policy FROM service_request_types t WHERE t.id=type_id;
 IF policy IS NULL THEN RETURN NULL; END IF;
 SELECT to_jsonb(s)-'singleton' INTO calendar FROM labor_calendar_settings s WHERE singleton;
 RETURN policy||jsonb_build_object('calendar',calendar,'response_due_at',request_sla_due(started,(policy->>'response_minutes')::integer,calendar,policy->>'clock'='continuous'),'resolution_due_at',request_sla_due(started,(policy->>'resolution_minutes')::integer,calendar,policy->>'clock'='continuous'));
END $$;
CREATE FUNCTION request_sla_projection(snapshot jsonb, attended timestamptz, resolved timestamptz) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN snapshot IS NULL THEN NULL ELSE snapshot||jsonb_build_object(
 'response_status',CASE WHEN coalesce(attended,CURRENT_TIMESTAMP)>(snapshot->>'response_due_at')::timestamptz THEN 'breached' WHEN attended IS NOT NULL THEN 'met' ELSE 'running' END,
 'resolution_status',CASE WHEN coalesce(resolved,CURRENT_TIMESTAMP)>(snapshot->>'resolution_due_at')::timestamptz THEN 'breached' WHEN resolved IS NOT NULL THEN 'met' ELSE 'running' END) END
$$;
CREATE INDEX service_requests_report ON service_requests(created_at,type_id,criticality,product_id);
