INSERT INTO record (head, quantity) VALUES
	('Apple', -1),
	('Orange', 0),
	('Banana', 1);

INSERT INTO view (view_name, query) VALUES
	('View 1', 'SELECT * FROM record'),
	('View 2', 'SELECT * FROM view'),
	('View 3', 'SELECT head FROM record');

INSERT INTO configuration (quantity, configuration_name, views) VALUES
	(1, 'Configuration 1', '{"View 1": true, "View 2": false}'),
	(0, 'Configuration 2', '{"View 2": true, "View 1": true}'),
	(0, 'Configuration 3', '{"View 1": true, "View 2": false, "View 3": true}')

