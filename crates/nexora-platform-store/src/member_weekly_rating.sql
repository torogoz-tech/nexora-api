WITH settings AS (
 SELECT timezone, starts_on, (clock_timestamp() AT TIME ZONE timezone)::date today
 FROM labor_calendar_settings
), days AS (
 SELECT d::date date
 FROM settings s JOIN actors a ON a.id=$1 JOIN actor_roles r ON r.actor_id=a.id,
 LATERAL generate_series(date_trunc('week',s.today::timestamp),s.today::timestamp,interval '1 day') d
 WHERE d::date>=GREATEST(s.starts_on,(r.rating_started_at AT TIME ZONE s.timezone)::date,(a.created_at AT TIME ZONE s.timezone)::date)
 AND NOT developer_rating_excluded($1,d::date)
), daily AS (
 SELECT d.date,avg(e.score) score FROM days d LEFT JOIN developer_rating_entries e ON e.actor_id=$1 AND e.date=d.date GROUP BY d.date
)
SELECT jsonb_build_object('start',to_char(date_trunc('week',s.today::timestamp),'YYYY-MM-DD'),
 'end',to_char(date_trunc('week',s.today::timestamp)+interval '6 days','YYYY-MM-DD'),
 'score',(SELECT avg(score) FROM daily)) FROM settings s
