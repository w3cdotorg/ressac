-- Ressac schema v5: themes and territories for the sea chart (sub-project 6).
-- The core writes every column and table below; the apps read the chart through
-- `ressac chart --json`, never through these tables.
CREATE TABLE theme (
  id       INTEGER PRIMARY KEY,
  key      TEXT NOT NULL UNIQUE,      -- stable identifier, e.g. 'ai-labs'
  name     TEXT NOT NULL,             -- 'AI labs and tools'
  sea_name TEXT NOT NULL,             -- 'SEA OF THE LABS'
  position INTEGER NOT NULL           -- order round the harbour; neighbours touch
);

ALTER TABLE feed ADD COLUMN theme_id INTEGER REFERENCES theme(id) ON DELETE SET NULL;

CREATE TABLE territory (
  id       INTEGER PRIMARY KEY,
  name     TEXT NOT NULL UNIQUE,      -- 'LE MONDE GROUP'
  kind     TEXT NOT NULL CHECK (kind IN ('territory', 'continent')),
  theme_id INTEGER REFERENCES theme(id) ON DELETE CASCADE,  -- continents only: the whole theme
  CHECK ((kind = 'continent') = (theme_id IS NOT NULL))
);

CREATE TABLE territory_feed (
  territory_id INTEGER NOT NULL REFERENCES territory(id) ON DELETE CASCADE,
  feed_id      INTEGER NOT NULL REFERENCES feed(id) ON DELETE CASCADE,
  position     INTEGER NOT NULL,
  PRIMARY KEY (territory_id, feed_id)
);
