CREATE TABLE github_settings(
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),app_id text NOT NULL,client_id text NOT NULL,slug text NOT NULL,
 origin text NOT NULL,api_origin text NOT NULL,host text NOT NULL,redirect_uri text NOT NULL,enabled boolean NOT NULL,
 revision bigint NOT NULL CHECK(revision>0),secrets jsonb NOT NULL
);
CREATE TABLE github_consents(
 id text PRIMARY KEY REFERENCES git_authorizations(id),remote_account_id text NOT NULL,remote_login text NOT NULL,
 tokens jsonb NOT NULL,token_expires_at timestamptz NOT NULL,expires_at timestamptz NOT NULL,connection_id text REFERENCES git_connections(id)
);
ALTER TABLE git_connections ADD COLUMN installation_id text;
ALTER TABLE git_connections ADD CONSTRAINT github_installation_identity CHECK(provider <> 'github' OR installation_id IS NOT NULL);
CREATE INDEX github_connection_installation ON git_connections(installation_id) WHERE provider='github';
