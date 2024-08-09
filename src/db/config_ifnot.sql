INSERT INTO configuration (save_mode) SELECT ('Automatic') WHERE NOT EXISTS ( SELECT 1 FROM configuration);
