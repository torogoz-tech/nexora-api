CREATE TABLE user_avatars (
 actor_id text PRIMARY KEY REFERENCES users(actor_id),
 revision bigint NOT NULL CHECK(revision>0),
 image_png bytea CHECK(image_png IS NULL OR octet_length(image_png)<=2097152)
);
