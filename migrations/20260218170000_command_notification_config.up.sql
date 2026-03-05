ALTER TABLE configuration
ADD COLUMN show_command_notifications INTEGER NOT NULL DEFAULT 0;

ALTER TABLE configuration
ADD COLUMN command_notification_seconds REAL NOT NULL DEFAULT -1;
