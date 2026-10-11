-- Ressac schema v2: reading state for the apps, full text and search (sub-project 2a).
ALTER TABLE feed ADD COLUMN prefer_full_text INTEGER NOT NULL DEFAULT 0;
ALTER TABLE feed_health ADD COLUMN needs_triage INTEGER NOT NULL DEFAULT 0;

CREATE TABLE entry_fulltext (
  entry_id   INTEGER PRIMARY KEY REFERENCES entry(id),
  fetched_at TEXT NOT NULL,
  html       TEXT,
  title      TEXT,
  byline     TEXT,
  error      TEXT,
  CHECK ((html IS NULL) <> (error IS NULL))
);

-- rowid = entry.id. Kept up to date by the core (no triggers: HTML to text needs Rust).
CREATE VIRTUAL TABLE entry_fts USING fts5(title, author, body, tokenize = 'unicode61 remove_diacritics 2');

CREATE INDEX entry_unread ON entry(feed_id) WHERE read_at IS NULL;
CREATE INDEX entry_starred ON entry(starred_at) WHERE starred_at IS NOT NULL;

-- Contract views for the apps.
CREATE VIEW v_entry AS
SELECT e.id AS entry_id, e.feed_id, f.title AS feed_title, f.folder_id,
       e.title, e.author, e.url, e.published_at,
       (e.read_at IS NOT NULL) AS is_read, (e.starred_at IS NOT NULL) AS is_starred,
       EXISTS (SELECT 1 FROM entry_fulltext t WHERE t.entry_id = e.id AND t.html IS NOT NULL) AS has_fulltext,
       e.summary_html, e.content_html
FROM entry e JOIN feed f ON f.id = e.feed_id
WHERE f.state = 'active';

CREATE VIEW v_unread_count AS
SELECT f.id AS feed_id, COUNT(e.id) AS unread
FROM feed f LEFT JOIN entry e ON e.feed_id = f.id AND e.read_at IS NULL
WHERE f.state = 'active'
GROUP BY f.id;

CREATE VIEW v_garden_row AS
SELECT f.id AS feed_id, f.title, fp.path AS folder_path, f.state,
       h.status, h.reason, json_extract(h.evidence, '$.clue') AS clue,
       h.evidence, h.remarks, h.proposed_action, COALESCE(h.needs_triage, 0) AS needs_triage,
       (SELECT d.decision FROM garden_decision d WHERE d.feed_id = f.id ORDER BY d.id DESC LIMIT 1) AS last_decision,
       f.wayback_url, f.html_url, f.xml_url
FROM feed f
LEFT JOIN v_folder_path fp ON fp.folder_id = f.folder_id
LEFT JOIN feed_health h ON h.feed_id = f.id;

UPDATE schema_meta SET version = 2;
