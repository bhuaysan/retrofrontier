DROP INDEX IF EXISTS idx_metadata_jobs_bulk_run;
ALTER TABLE metadata_jobs DROP COLUMN bulk_run_id;
DROP INDEX IF EXISTS idx_metadata_scrape_run_items_state;
DROP TABLE IF EXISTS metadata_scrape_run_items;
DROP INDEX IF EXISTS idx_metadata_scrape_runs_recent;
DROP INDEX IF EXISTS idx_metadata_scrape_runs_active;
DROP TABLE IF EXISTS metadata_scrape_runs;
