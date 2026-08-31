DELETE FROM record_extension WHERE namespace = 'lince.organ';

UPDATE record SET body = '' WHERE kind = 'organ' AND slug = 'local-organ';
