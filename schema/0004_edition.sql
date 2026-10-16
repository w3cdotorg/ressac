-- Ressac schema v4: rhythm groups and the morning edition (sub-project 3).
-- The core writes every column and table below; the apps read them through the v_ views.
ALTER TABLE feed ADD COLUMN cadence_override TEXT
  CHECK (cadence_override IN ('daily', 'weekly', 'monthly', 'rare', 'silent'));
ALTER TABLE feed_health ADD COLUMN cadence_group TEXT NOT NULL DEFAULT 'silent';

CREATE INDEX entry_fetched ON entry(fetched_at);

CREATE TABLE edition (
  id           INTEGER PRIMARY KEY,
  built_at     TEXT NOT NULL,
  window_start TEXT NOT NULL,
  window_end   TEXT NOT NULL,
  unreachable  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE edition_entry (
  edition_id INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  entry_id   INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
  section    TEXT NOT NULL CHECK (section IN ('front', 'week', 'wire')),
  rank       INTEGER NOT NULL,
  PRIMARY KEY (edition_id, entry_id)
);

CREATE TABLE edition_overflow (
  edition_id   INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  feed_id      INTEGER NOT NULL REFERENCES feed(id) ON DELETE CASCADE,
  section      TEXT NOT NULL CHECK (section IN ('front', 'week', 'wire')),
  hidden_count INTEGER NOT NULL,
  PRIMARY KEY (edition_id, feed_id)
);

-- Contract view: the rhythm of every active feed with a diagnosis.
CREATE VIEW v_cadence AS
  SELECT d.id AS feed_id, d.title, d.page, p.path AS folder_path,
         COALESCE(d.cadence_override, h.cadence_group) AS effective_group,
         h.cadence_group AS computed_group,
         d.cadence_override IS NOT NULL AS is_override,
         h.median_interval_days, h.entries_per_month, h.last_entry_at,
         (SELECT COUNT(*) FROM entry e WHERE e.feed_id = d.id AND e.read_at IS NULL) AS unread
  FROM feed d JOIN feed_health h ON h.feed_id = d.id
  LEFT JOIN v_folder_path p ON p.folder_id = d.folder_id
  WHERE d.state = 'active';

-- Contract view: the latest edition, one row per entry, with live read and starred state.
-- Frozen: a feed buried after the build keeps its entries here.
CREATE VIEW v_edition_today AS
  SELECT ed.id AS edition_id, ed.built_at, ed.window_start, ed.window_end, ed.unreachable,
         ee.entry_id, ee.section, ee.rank, e.feed_id, d.title AS feed_title,
         e.title, e.published_at, e.read_at, e.starred_at
  FROM edition ed
  JOIN edition_entry ee ON ee.edition_id = ed.id
  JOIN entry e ON e.id = ee.entry_id
  JOIN feed d ON d.id = e.feed_id
  WHERE ed.id = (SELECT MAX(id) FROM edition);

-- Contract view: what the caps of the latest edition left in each feed.
CREATE VIEW v_edition_overflow AS
  SELECT o.edition_id, o.section, o.feed_id, d.title AS feed_title, d.page, o.hidden_count
  FROM edition_overflow o JOIN feed d ON d.id = o.feed_id
  WHERE o.edition_id = (SELECT MAX(id) FROM edition);

UPDATE schema_meta SET version = 4;
