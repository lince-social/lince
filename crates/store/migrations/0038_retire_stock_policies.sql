UPDATE effect_queue
SET status = 'cancelled', result = 'Stock policy retired; configure ordinary Karma'
WHERE status IN ('queued', 'running', 'uncertain')
  AND (kind = 'refresh-stock-policy' OR instr(payload, 'refresh-stock-policy') > 0);

UPDATE recurrence
SET state = 'paused', quantity = 0,
    consequences_json = '[{"kind":"notify","message":"Stock policy retired; configure ordinary Karma"}]'
WHERE EXISTS (
    SELECT 1 FROM json_each(recurrence.consequences_json)
    WHERE json_extract(value, '$.kind') = 'refresh-stock-policy'
);
