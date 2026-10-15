-- Ressac schema v3: teletext page numbers for the Antiope theme (sub-project 4).
-- Folders and feeds share one number space; the core enforces it across both tables.
ALTER TABLE folder ADD COLUMN page INTEGER;
ALTER TABLE feed ADD COLUMN page INTEGER;
CREATE UNIQUE INDEX folder_page ON folder(page);
CREATE UNIQUE INDEX feed_page ON feed(page);

-- Contract view for the apps: every numbered folder and feed, buried feeds included.
CREATE VIEW v_page AS
  SELECT f.page, 'folder' AS kind, f.id AS ref_id, f.name AS title,
         (SELECT p.page FROM folder p WHERE p.id = f.parent_id) AS parent_page
  FROM folder f WHERE f.page IS NOT NULL
  UNION ALL
  SELECT d.page, 'feed', d.id, d.title,
         (SELECT p.page FROM folder p WHERE p.id = d.folder_id)
  FROM feed d WHERE d.page IS NOT NULL;

UPDATE schema_meta SET version = 3;
