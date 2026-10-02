CREATE INDEX effect_queue_active ON effect_queue(status) WHERE status IN ('queued', 'running');
