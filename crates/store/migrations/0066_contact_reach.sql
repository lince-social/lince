UPDATE organ_contact SET mode = 'auto' WHERE mode NOT IN ('direct', 'mailbox');
