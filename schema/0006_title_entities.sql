-- Ressac schema v6: no structural change. Some feeds encode titles twice, so entry and feed
-- titles stored until now may still hold an HTML entity (`America&#39;s`). The migration step
-- in Rust (`store::fetch::decode_stored_titles`) decodes them once, in the same transaction
-- as the version bump: a crash leaves the database at v5 with every title untouched.
-- New titles are decoded at ingest (`parse::decode_title_entities`), so it must never run again.
SELECT 1;
