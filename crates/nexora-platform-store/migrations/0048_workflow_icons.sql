-- Display metadata belongs to the current definition, never immutable versions.
ALTER TABLE workflows ADD COLUMN icon text NOT NULL DEFAULT 'workflow'
 CHECK(icon IN ('workflow','git-branch','code','bug','shield-check','rocket','layers','list-todo','briefcase','lightbulb','flask-conical','git-pull-request'));
