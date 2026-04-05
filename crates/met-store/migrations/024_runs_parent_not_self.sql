-- A run cannot be its own parent (defensive; API should never set this).

ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_parent_not_self;
ALTER TABLE runs ADD CONSTRAINT runs_parent_not_self CHECK (parent_run_id IS NULL OR parent_run_id <> id);
