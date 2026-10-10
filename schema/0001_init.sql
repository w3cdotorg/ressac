-- Ressac schema v1. Timestamps: RFC 3339 UTC text with milliseconds and 'Z'.
CREATE TABLE schema_meta (version INTEGER NOT NULL);
INSERT INTO schema_meta (version) VALUES (1);

CREATE TABLE folder (
  id        INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES folder(id),
  name      TEXT NOT NULL,
  position  INTEGER NOT NULL
);

CREATE TABLE feed (
  id                   INTEGER PRIMARY KEY,
  folder_id            INTEGER REFERENCES folder(id),
  position             INTEGER NOT NULL,
  title                TEXT NOT NULL,
  xml_url              TEXT NOT NULL,
  xml_url_original     TEXT NOT NULL,
  html_url             TEXT,
  url_key              TEXT NOT NULL,
  final_url            TEXT,
  description_original TEXT,
  description_current  TEXT,
  etag                 TEXT,
  last_modified        TEXT,
  state                TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'buried')),
  merged_into_id       INTEGER REFERENCES feed(id),
  opml_attrs           TEXT NOT NULL DEFAULT '{}',
  https_available      INTEGER,
  discovered_url       TEXT,
  discovery_checked_at TEXT,
  wayback_url          TEXT,
  added_at             TEXT NOT NULL
);
CREATE UNIQUE INDEX feed_url_key_active ON feed(url_key) WHERE state = 'active';

CREATE TABLE fetch_log (
  id              INTEGER PRIMARY KEY,
  feed_id         INTEGER NOT NULL REFERENCES feed(id),
  run_id          TEXT NOT NULL,
  at              TEXT NOT NULL,
  http_status     INTEGER,
  error_kind      TEXT,             -- JSON of model::ErrorKind, e.g. "Dns" or {"Http":404}
  redirect_chain  TEXT NOT NULL DEFAULT '[]',
  content_type    TEXT,
  bytes           INTEGER NOT NULL DEFAULT 0,
  duration_ms     INTEGER NOT NULL DEFAULT 0,
  entry_count     INTEGER,
  not_modified    INTEGER NOT NULL DEFAULT 0,
  network_suspect INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX fetch_log_feed_at ON fetch_log(feed_id, at DESC);

CREATE TABLE entry (
  id                       INTEGER PRIMARY KEY,
  feed_id                  INTEGER NOT NULL REFERENCES feed(id),
  guid                     TEXT NOT NULL,
  url                      TEXT,
  title                    TEXT,
  author                   TEXT,
  published_at             TEXT NOT NULL,
  published_at_is_fallback INTEGER NOT NULL DEFAULT 0,
  updated_at               TEXT,
  summary_html             TEXT,
  content_html             TEXT,
  content_kind             TEXT,
  read_at                  TEXT,
  starred_at               TEXT,
  fetched_at               TEXT NOT NULL,
  UNIQUE (feed_id, guid)
);
CREATE INDEX entry_feed_published ON entry(feed_id, published_at DESC);

CREATE TABLE feed_health (
  feed_id              INTEGER PRIMARY KEY REFERENCES feed(id),
  status               TEXT NOT NULL,
  reason               TEXT NOT NULL,
  evidence             TEXT NOT NULL DEFAULT '{}',
  remarks              TEXT NOT NULL DEFAULT '[]',
  proposed_action      TEXT,
  hijack_score         REAL NOT NULL DEFAULT 0,
  last_entry_at        TEXT,
  median_interval_days REAL,
  entries_per_month    REAL NOT NULL DEFAULT 0,
  computed_at          TEXT NOT NULL
);

CREATE TABLE garden_decision (
  id                 INTEGER PRIMARY KEY,
  feed_id            INTEGER NOT NULL REFERENCES feed(id),
  decision           TEXT NOT NULL CHECK (decision IN ('keep', 'repair', 'bury')),
  status_at_decision TEXT NOT NULL,
  action             TEXT,
  previous           TEXT NOT NULL DEFAULT '{}',
  decided_at         TEXT NOT NULL,
  note               TEXT
);
CREATE INDEX garden_decision_feed ON garden_decision(feed_id, id DESC);

-- Contract views (stable API for other clients).
CREATE VIEW v_folder_path AS
WITH RECURSIVE p(id, path) AS (
  SELECT id, name FROM folder WHERE parent_id IS NULL
  UNION ALL
  SELECT f.id, p.path || ' / ' || f.name FROM folder f JOIN p ON f.parent_id = p.id
)
SELECT id AS folder_id, path FROM p;

CREATE VIEW v_feed_overview AS
SELECT f.id AS feed_id,
       f.title,
       f.xml_url,
       f.html_url,
       f.state,
       fp.path AS folder_path,
       h.status,
       h.reason,
       h.last_entry_at,
       (SELECT d.decision FROM garden_decision d
         WHERE d.feed_id = f.id ORDER BY d.id DESC LIMIT 1) AS decision
FROM feed f
LEFT JOIN v_folder_path fp ON fp.folder_id = f.folder_id
LEFT JOIN feed_health h ON h.feed_id = f.id;
