-- Track how many heartbeats passed without the agent reporting draining while drain was requested.
ALTER TABLE agents ADD COLUMN IF NOT EXISTS drain_missed_heartbeats INT NOT NULL DEFAULT 0;

COMMENT ON COLUMN agents.drain_missed_heartbeats IS
    'Incremented while status=draining and agent heartbeat still reports non-draining; reset when agent acks draining.';
