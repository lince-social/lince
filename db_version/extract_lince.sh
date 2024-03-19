#!/bin/bash

# Define the base filename
base_filename="old_lince_postgre"

# Define the extension
extension=".sql"

# Initialize the counter
counter=1

# Construct the filename with the counter
filename="${base_filename}_${counter}${extension}"

# Check if the file exists and increment the counter until a non-existing filename is found
while [ -f "$filename" ]; do
    let counter++
    filename="${base_filename}_${counter}${extension}"
done

# Now, filename contains the next available filename
# Proceed with the pg_dump command
pg_dump -U postgres -d lince -f "$filename"

echo "Backup created: $filename"

psql -U postgres -f postgre.sql
psql -U postgres -d lince
