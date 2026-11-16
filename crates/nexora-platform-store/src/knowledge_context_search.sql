WITH sources AS (
 SELECT c.id,c.project_id,c.source_kind,c.title,c.content,c.source_revision,NULL,
        'knowledge_chunk'::text,c.content_hash,c.id,c.published
 FROM knowledge_chunks c
 WHERE c.published AND (c.project_id=$1 OR (c.project_id IS NULL AND $2))
 UNION ALL
 SELECT e.snapshot_id||':'||e.path||':'||e.sequence,e.project_id,'code_fact',e.title,e.content,NULL,NULL,
        'code_snapshot'::text,e.content_hash,e.snapshot_id,e.published
 FROM code_snapshot_embeddings e
 WHERE e.published AND e.project_id=$1
), matches AS (
 SELECT sources.*,
        to_tsvector('simple',title||' '||body) AS terms,
        plainto_tsquery('simple',$3) AS query,
        ts_rank_cd(to_tsvector('simple',title||' '||body),plainto_tsquery('simple',$3)) AS lexical_score,
        CASE WHEN strpos(lower(title||' '||body),lower($3))>0 THEN 1.0 ELSE 0.0 END AS exact_score,
        COALESCE(embedding_match.semantic_score,0.0) AS semantic_score,
        CASE WHEN source_type='knowledge_chunk' THEN LEAST(1.0,COALESCE((SELECT jsonb_array_length(s.structure->'relationships')::numeric/4.0 FROM knowledge_structures s WHERE s.resource_id=sources.id AND s.source_revision=sources.revision),0.0)) ELSE 0.0 END AS graph_score
 FROM sources
 LEFT JOIN LATERAL (
   SELECT (1.0/(1.0+(candidate.embedding <=> ($4::text)::vector)))::double precision AS semantic_score
   FROM (
     SELECT e.embedding
     FROM knowledge_embeddings e
     JOIN knowledge_chunks c ON c.id=e.chunk_id
     WHERE sources.source_type='knowledge_chunk' AND c.id=sources.id AND e.published
       AND e.dimension=vector_dims(($4::text)::vector)
       AND ($5::text IS NULL OR e.embedding_model=$5)
     UNION ALL
     SELECT e.embedding
     FROM code_snapshot_embeddings e
     WHERE sources.source_type='code_snapshot' AND e.snapshot_id=sources.version_id AND e.path=sources.title AND e.project_id=$1 AND e.published
       AND e.dimension=vector_dims(($4::text)::vector)
       AND ($5::text IS NULL OR e.embedding_model=$5)
   ) candidate
   WHERE $4::text IS NOT NULL AND ($5::text IS NULL OR $5<>'')
   ORDER BY candidate.embedding <=> ($4::text)::vector
   LIMIT 1
 ) embedding_match ON true
)
SELECT jsonb_build_object('id',id,'project_id',project_id,'kind',kind,'title',title,
 'revision',revision,'source_revision',source_revision,'source_type',source_type,'source_checksum',source_checksum,'version_id',version_id,'published',published,
 'excerpt',left(body,1600),'text_truncated',length(body)>1600,'untrusted_content',true,
 'score',round((LEAST(1.0,lexical_score)*0.25+exact_score*0.20+semantic_score*0.40+graph_score*0.15)::numeric,6),
 'score_components',jsonb_build_object('lexical',round(LEAST(1.0,lexical_score)::numeric,6),'exact',exact_score,'semantic',round(semantic_score::numeric,6),'graph',graph_score),
 'retrieval_channels',jsonb_build_array(CASE WHEN lexical_score>0 THEN 'lexical' END,CASE WHEN exact_score>0 THEN 'exact' END,CASE WHEN semantic_score>0 THEN 'semantic' END,CASE WHEN graph_score>0 THEN 'graph' END))
FROM matches WHERE terms @@ query OR exact_score>0 OR semantic_score>0.15
ORDER BY (LEAST(1.0,lexical_score)*0.25+exact_score*0.20+semantic_score*0.40+graph_score*0.15) DESC,lower(title),id LIMIT 21
