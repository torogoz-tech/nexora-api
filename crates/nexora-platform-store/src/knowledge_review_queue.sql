WITH reviews AS (
 SELECT to_jsonb(r)||jsonb_build_object('current_revision',CASE r.kind
 WHEN 'project_document' THEN (SELECT revision FROM project_documents WHERE id=r.resource_id AND NOT archived)
 WHEN 'organization_document' THEN (SELECT revision FROM organization_documents WHERE id=r.resource_id)
 WHEN 'contribution' THEN (SELECT revision FROM knowledge_contributions WHERE id=r.resource_id AND status='ready')
 WHEN 'feature' THEN (SELECT revision FROM project_features WHERE id=r.resource_id AND NOT archived) END) AS item,r.created_at,r.id
 FROM knowledge_reviews r
 WHERE ($1='' OR r.status=$1) AND ($2::text IS NULL OR r.resource_id=$2)
 AND (r.project_id=ANY($3) OR (r.project_id IS NULL AND $4))
 UNION ALL
 SELECT jsonb_build_object('id',f.id,'kind','feature_proposal','resource_id',f.id,'project_id',f.project_id,'title',f.requirement_title,'document_kind','feature','status',f.status,'resource_revision',f.revision,'current_revision',f.revision,'revision',f.revision),f.created_at,f.id
 FROM feature_proposals f WHERE ($1='' OR f.status=$1) AND $2::text IS NULL AND f.project_id=ANY($3)
), page AS (SELECT * FROM reviews ORDER BY created_at DESC,id LIMIT 20 OFFSET $5)
SELECT jsonb_build_object('items',COALESCE((SELECT jsonb_agg(item ORDER BY created_at DESC,id) FROM page),'[]'::jsonb),'total',(SELECT count(*) FROM reviews))
