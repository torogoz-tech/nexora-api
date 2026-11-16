WITH filtered AS (
 SELECT r.*,a.name AS requester_name,t.name AS type_name,t.icon AS type_icon,t.color AS type_color,p.name AS product_name,
 request_sla_projection(r.sla_snapshot,r.first_attention_at,r.resolved_at) AS sla
 FROM service_requests r JOIN actors a ON a.id=r.requester_id JOIN service_request_types t ON t.id=r.type_id LEFT JOIN products p ON p.id=r.product_id
 CROSS JOIN labor_calendar_settings c
 WHERE ($1 OR r.requester_id=$2)
 AND ($3='' OR (r.created_at AT TIME ZONE c.timezone)::date>=nullif($3,'')::date)
 AND ($4='' OR (r.created_at AT TIME ZONE c.timezone)::date<=nullif($4,'')::date)
 AND ($5='' OR ($5='none' AND r.product_id IS NULL) OR r.product_id=$5) AND ($6='' OR r.type_id=$6)
 AND ($7='' OR r.criticality=$7) AND ($8='' OR r.requester_id=$8) AND ($9='' OR r.status=$9)
), items AS (SELECT * FROM filtered ORDER BY created_at DESC,id LIMIT CASE WHEN $11 THEN NULL ELSE 20 END OFFSET CASE WHEN $11 THEN 0 ELSE $10::bigint END),
products AS (SELECT product_id,coalesce(product_name,'Sin producto') AS name,count(*) AS total,count(*) FILTER(WHERE status<>'completed') AS pending FROM filtered GROUP BY product_id,product_name),
types AS (SELECT type_id,type_name AS name,count(*) AS total FROM filtered GROUP BY type_id,type_name),
requesters AS (SELECT requester_id AS id,a.name AS name FROM service_requests r JOIN actors a ON a.id=r.requester_id WHERE ($1 OR r.requester_id=$2) GROUP BY requester_id,a.name)
SELECT jsonb_build_object('total',(SELECT count(*) FROM filtered),
 'summary',(SELECT jsonb_build_object('new',count(*) FILTER(WHERE status='new'),'in_progress',count(*) FILTER(WHERE status='in_progress'),'completed',count(*) FILTER(WHERE status='completed'),
 'response_breached',count(*) FILTER(WHERE sla->>'response_status'='breached'),'resolution_breached',count(*) FILTER(WHERE sla->>'resolution_status'='breached'),
 'sla_count',count(*) FILTER(WHERE sla IS NOT NULL),'response_average_minutes',avg(extract(epoch FROM first_attention_at-created_at)/60),'resolution_average_minutes',avg(extract(epoch FROM resolved_at-created_at)/60)) FROM filtered),
 'items',coalesce((SELECT jsonb_agg(to_jsonb(items)-'body'-'sla_snapshot' ORDER BY created_at DESC,id) FROM items),'[]'::jsonb),
 'products',coalesce((SELECT jsonb_agg(to_jsonb(products) ORDER BY lower(name),product_id) FROM products),'[]'::jsonb),
 'types',coalesce((SELECT jsonb_agg(to_jsonb(types) ORDER BY lower(name),type_id) FROM types),'[]'::jsonb),
 'requesters',coalesce((SELECT jsonb_agg(to_jsonb(requesters) ORDER BY lower(name),id) FROM requesters),'[]'::jsonb),
 'oldest_pending',coalesce((SELECT jsonb_agg(row) FROM (SELECT id,number,title,created_at FROM filtered WHERE status<>'completed' ORDER BY created_at,id LIMIT 5) row),'[]'::jsonb))
