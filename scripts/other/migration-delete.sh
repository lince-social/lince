#!/usr/bin/env bash

set -euo pipefail

shopt -s nullglob

cd /root/git/lince-social/lince

systemctl list-unit-files --type=service --no-legend |
awk '/^lince.*\.service/ { print $1 }' |
while read -r service; do
    echo "Stopping $service"
    systemctl stop "$service"
done

git add .
git stash
git pull

for db in /root/.config/lince*/lince.db; do
    echo "Clearing migrations: $db"
    sqlite3 "$db" 'DELETE FROM _sqlx_migrations;'
done

systemctl list-unit-files --type=service --no-legend |
awk '/^lince.*\.service/ { print $1 }' |
while read -r service; do
    echo "Restarting $service"
    systemctl restart "$service"
done
