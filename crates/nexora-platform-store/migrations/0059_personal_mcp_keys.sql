ALTER TABLE api_keys ADD COLUMN personal_mcp boolean NOT NULL DEFAULT false,
 ADD COLUMN last_mcp_at timestamptz,
 ADD COLUMN mcp_client_name text CHECK(mcp_client_name IS NULL OR octet_length(mcp_client_name)<=128),
 ADD COLUMN mcp_client_version text CHECK(mcp_client_version IS NULL OR octet_length(mcp_client_version)<=64);
