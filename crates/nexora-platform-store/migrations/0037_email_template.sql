CREATE TABLE installation_email_template (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 html text,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision > 0),
 CHECK(html IS NULL OR octet_length(html) <= 100000)
);
INSERT INTO installation_email_template(singleton) VALUES(true);
