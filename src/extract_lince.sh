#!/bin/bash

default_user="postgres"
default_db="lince"
base_filename="lincedb"
extension=".sql"
counter=0

read -p "Enter username (default: $default_user): " user
read -p "Enter database name (default: $default_db): " db

user=${user:-$default_user}
db=${db:-$default_db}

if [ $(psql -U "$user" -lqt | cut -d \| -f 1 | grep -qw "$db" && echo 1 || echo 0) -eq 0 ]; then
    echo "Database '$db' does not exist."
    exit 1
fi

# Construct the filename
filename="${base_filename}_${counter}${extension}"

# Check if the file already exists and increment the counter until a non-existing filename is found
while [ -f "$filename" ]; do
    let counter++
    filename="${base_filename}_${counter}${extension}"
done

# Dump the database
pg_dump -U "$user" -d "$db" -f "$filename"

echo "Backup created: $filename"

# Restore the database (assuming postgre.sql is the file you want to restore)
psql -U "$user" -d "$db" -f postgre.sql
