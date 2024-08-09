INSERT INTO configuration (save_mode) SELECT ('Automatic') WHERE NOT EXISTS ( SELECT 1 FROM configuration);
INSERT INTO views (view) SELECT ('SELECT * FROM record') WHERE NOT EXISTS ( SELECT 1 FROM views);
