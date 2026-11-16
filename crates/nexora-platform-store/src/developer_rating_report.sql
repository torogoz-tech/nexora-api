WITH days AS (
 SELECT d::date date, developer_rating_excluded($1,d::date) excluded
 FROM labor_calendar_settings s JOIN actors a ON a.id=$1 JOIN actor_roles r ON r.actor_id=a.id,
 LATERAL generate_series(make_date($2,1,1)::timestamp,make_date($2,12,31)::timestamp,interval '1 day') d
 WHERE d::date>=GREATEST(s.starts_on,(r.rating_started_at AT TIME ZONE s.timezone)::date,(a.created_at AT TIME ZONE s.timezone)::date)
 AND d::date<=(clock_timestamp() AT TIME ZONE s.timezone)::date
), daily AS (
 SELECT d.date,d.excluded,CASE WHEN d.excluded THEN NULL ELSE avg(e.score) END score,
 EXISTS(SELECT 1 FROM developer_push_activity p CROSS JOIN labor_calendar_settings s WHERE p.actor_id=$1 AND p.occurred_at>=(d.date::timestamp AT TIME ZONE s.timezone) AND p.occurred_at<((d.date+1)::timestamp AT TIME ZONE s.timezone)) push,
 EXISTS(SELECT 1 FROM git_commit_authorship g CROSS JOIN labor_calendar_settings s WHERE g.actor_id=$1 AND g.occurred_at>=(d.date::timestamp AT TIME ZONE s.timezone) AND g.occurred_at<((d.date+1)::timestamp AT TIME ZONE s.timezone)) authored,
 COALESCE(jsonb_agg((to_jsonb(e)-'actor_id'-'date')||jsonb_build_object('created_by_name',(SELECT name FROM actors WHERE id=e.created_by),'attachments',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',f.id,'name',f.name,'image',f.image,'size_bytes',octet_length(f.payload)) ORDER BY f.id) FROM rating_evidence f WHERE f.rating_id=e.id),'[]'::jsonb)) ORDER BY e.created_at,e.id) FILTER(WHERE e.id IS NOT NULL),'[]'::jsonb) entries
 FROM days d LEFT JOIN developer_rating_entries e ON e.actor_id=$1 AND e.date=d.date
 GROUP BY d.date,d.excluded
), weeks AS (
 SELECT to_char(date,'IYYY-"W"IW') period,avg(score) score FROM daily GROUP BY 1
), month_weeks AS (
 SELECT to_char(date,'YYYY-MM') month_key,to_char(date,'IYYY-"W"IW') week_key,avg(score) score FROM daily GROUP BY 1,2
), months AS (
 SELECT month_key period,avg(score) score FROM month_weeks GROUP BY 1
)
SELECT jsonb_build_object('actor_id',$1::text,'year',$2::integer,
 'daily',COALESCE((SELECT jsonb_agg(to_jsonb(daily) ORDER BY date) FROM daily),'[]'::jsonb),
 'weekly',COALESCE((SELECT jsonb_agg(to_jsonb(weeks) ORDER BY period) FROM weeks WHERE score IS NOT NULL),'[]'::jsonb),
 'monthly',COALESCE((SELECT jsonb_agg(to_jsonb(months) ORDER BY period) FROM months WHERE score IS NOT NULL),'[]'::jsonb),
 'annual',(SELECT avg(score) FROM months))
