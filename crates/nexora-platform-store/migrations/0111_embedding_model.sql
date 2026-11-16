UPDATE installation_services
SET settings = settings || '{"embedding_model":"text-embedding-3-small"}'::jsonb
WHERE kind='openai' AND NOT (settings ? 'embedding_model');
