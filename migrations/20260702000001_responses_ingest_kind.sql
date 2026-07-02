-- Which ingest-ladder rung produced this response's prose
-- (bot-lifecycle spec Part 2). NULL for abstention / carry-forward rows
-- and for rows predating lenient ingest.
-- Values: 'clean' | 'salvaged_field' | 'salvaged_raw' | 'truncated'
-- (closed set guarded by sentinel ING-001).
ALTER TABLE responses ADD COLUMN ingest_kind TEXT NULL;
