INSERT INTO record (head, quantity) VALUES
	('Adicionar rows em qualquer table', -1),
	('Deletar rows em qualquer table', -1),
	('Adicionar configuraçoes com botao', -1),
	('Adicionar views com botao', -1);

INSERT INTO view (view_name, query) VALUES
	('View 1', 'SELECT * FROM record'),
	('View 2', 'SELECT * FROM view'),
	('View 3', 'SELECT head FROM record');

INSERT INTO configuration (quantity, configuration_name) VALUES
	(1, 'Configuration 1'),
	(0, 'Configuration 2'),
	(0, 'Configuration 3');

INSERT INTO configuration_view (configuration_id, view_id, is_active) VALUES
(1, 1, true),
(1, 2, false),
(2, 2, true),
(2, 1, true),
(3, 1, true),
(3, 2, false),
(3, 3, true);
