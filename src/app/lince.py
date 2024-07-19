from dateutil.relativedelta import relativedelta
import subprocess
import sys
import os
from datetime import datetime, timedelta
from tabulate import tabulate
import pandas as pd
import psycopg2


def create_connection_object(host = 'localhost', user = 'postgres', database = 'lince', password = '1', port = '5432'):
    connection = psycopg2.connect( host = host, user = user, database = database, password = password, port = port)
    connection.autocommit = True
    return connection


def execute_sql_command(command=None, database='lince'):
    connection = create_connection_object(database=database)
    cursor = connection.cursor()
    cursor.execute(command)    

    if command.startswith("SELECT"):
        df = pd.DataFrame(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])
        connection.close()
        return df

    connection.close()
    return True


def check_exists_db():
    connection = psycopg2.connect( host = 'localhost', user = 'postgres', password = '1', port = '5432')
    connection.autocommit = True
    cursor = connection.cursor()
    cursor.execute("SELECT datname FROM pg_database WHERE datname = 'lince'")

    result = cursor.fetchone()
    connection.close()
    return result

def dump_db():
    return subprocess.run(['pg_dump', '-U', 'postgres', '--no-password', '-F', 'plain', '-f', 'src/db/versions/db_dump.sql', 'lince'], text=True, input='1\n')

def drop_db():
    return execute_sql_command(command='DROP DATABASE lince', database=None)

def create_db():
    connection = psycopg2.connect( host = 'localhost', user = 'postgres', password = '1', port = '5432')
    connection.autocommit = True
    cursor = connection.cursor()
    cursor.execute('CREATE DATABASE lince')

    connection.close()
    return True

def scheme_db():
    with open(os.path.abspath(os.path.join(__file__,"..", "..", "..", "src", "db", "postgre.sql")), 'r') as file: return execute_sql_command(command = file.read())

def restore_db():
    p = subprocess.Popen("psql -h 'localhost' -d 'lince' -U postgres < src/db/versions/db_dump.sql", shell=True, stdin=subprocess.PIPE)
    return p.communicate(b"1\n")
def print_help():
    pass


def clear_and_print_header():
    os.system('clear')
    return print('- Lince -')


def create_row(table):
    tablecolumns = execute_sql_command(command=f"SELECT * FROM {table} WHERE false")

    columns = "("
    row = "("

    n = 0

    for column in tablecolumns:
        value = input(f'Value for {column} (if wanted): ')

        if value != "":

            if n != 0:
                row += ', '
                columns += ', '

            n = 1

            if column == 'quantidade':
                row += value
            else:
                row += "'" + value + "'"
            columns += column

    row += ')'
    columns += ')'

    return execute_sql_command(f'INSERT INTO {table} {columns} VALUES {row}')


def truncate_description(desc, max_length=150):
    if len(desc) > max_length:
        return desc[:max_length] + '...'
    return desc

def read_rows(table, limit = 0, order=False):
    command = f'SELECT * FROM {table}'

    if table == 'record':
        command += f" ORDER BY title ASC"

    if limit is False:
        return execute_sql_command(command=command)

    if limit <= 0:
        limit = input(f'Number of rows to fetch from {table} (no input fetches all): ')
    
    if isinstance(limit, int) or isinstance(limit, float):
        rows = int(limit)
        command += f" LIMIT {rows}"

    print()
    print(table)
    rows = execute_sql_command(command=command)

    if isinstance(rows, pd.DataFrame):
        if 'description' in rows.columns:
            rows['description'] = rows['description'].apply(truncate_description)
    elif rows and isinstance(rows, list):
        for row in rows:
            if 'description' in row:
                row['description'] = truncate_description(row['description'])

    return rows


def update_rows(table, set_clause=None, where_clause=None):
    if set_clause == None: set_clause = input("Set clause: ")
    if set_clause != "": command=f'UPDATE {table} SET {set_clause}'

    if where_clause == None: where_clause = input("Where clause: ")
    if where_clause != "": command += f' WHERE {where_clause}'

    return execute_sql_command(command=command)


def delete_rows(table):
    command = f'DELETE FROM {table}'

    where_clause = input("Where clause (no input deletes all): ")

    if where_clause != "":
        command += f' WHERE {where_clause}'

    return execute_sql_command(command=command)


def execute_sql_command_from_file():
    file_path = os.path.realpath(os.path.join(__file__, '..', '..', '..', input('File path starting from the lince dir: ')))

    with open(file_path, 'r') as file:
        return execute_sql_command(command = file.read())


def execute_frequency_job():
    record_df = read_rows('record', limit=False)

    frequency_df = read_rows('frequency', limit=False)
    frequency_df['next_date'] = pd.to_datetime(frequency_df['next_date'])

    today = datetime.today().date()

    for index, frequency_row in frequency_df.iterrows():

        frequency_record_id_reference = frequency_row['record_id']
        record_df_quantity = record_df.loc[record_df['id'] == frequency_record_id_reference, 'quantity']
        record_df_quantity = record_df_quantity.iloc[0]
        next_date = frequency_row['next_date'].date()
        today_datetime = pd.to_datetime(today).date()

        if frequency_row['finish_date'] == None:
            None
        elif today > frequency_row['finish_date']:
            continue
        if next_date > today_datetime:
            continue
        elif frequency_row['when_done'] == True and record_df_quantity < 0:
            continue
        elif frequency_row['times'] == 0:
            continue
        elif frequency_row['times'] < 0:
            times = frequency_row['times'] + 1
            update_rows('frequency', set_clause=f"times = {times}", where_clause=f"id = {frequency_row['id']}")

        next_date += relativedelta(months=frequency_row['months']) + timedelta(days=frequency_row['days'], seconds=frequency_row['seconds'])

        if today == next_date:
            next_date += timedelta(days=1)

        if pd.isna(frequency_row['day_week']):
            pass
        else:
            while next_date.isoweekday() not in [int(i) for i in str(int(frequency_row['day_week']))]:
                next_date += timedelta(days=1)

        record_df_quantity += frequency_row['delta']

        update_rows('frequency', set_clause=f"next_date = '{next_date}'", where_clause=f'id = {frequency_row["id"]}')
        update_rows(table='record', set_clause=f'quantity = {record_df_quantity}', where_clause=f'id = {frequency_record_id_reference}')

    return True


def choose_operation():
    options = [
        [ 'App', 'Operations', 'Tables' ],
        [ '[E] Exit', '[C] Create', '[1] Record' ],
        [ '[S] Save DB', '[R] Read', '[2] Frequency' ],
        [ '[L] Load DB', '[U] Update', '' ],
        [ '[H] Help', '[D] Delete', '' ],
        [ '', '[Q] Query', '' ],
        [ '', '[F] SQL File','' ]
    ]

    print()
    print('Menu')
    print(tabulate(options, headers='firstrow', tablefmt='psql'))

    return input('Your choice: ')


def execute_operation(operation):
    if ('e' or 'E') in operation:
        sys.exit()
    if ('s' or 'S') in operation:
        dump_db()
    if ('l' or 'L') in operation:
        restore_db()
    if ('h' or 'H') in operation:
        print_help()

    if '1' in operation:
        table = 'record'
    if '2' in operation:
        table = 'frequency'

    if ('c' or 'C') in operation:
        create_row(table)
    if ('r' or 'R') in operation:
        clear_and_print_header()
        print(table)
        print(tabulate(read_rows(table), headers='keys', tablefmt='psql'))
    if ('u' or 'U') in operation:
        update_rows(table)
    if ('d' or 'D') in operation:
        delete_rows(table)
    if ('q' or 'Q') in operation:
        execute_sql_command(command=input('SQL command: '))
    if ('f' or 'F') in operation:
        execute_sql_command_from_file()

def main():
    if check_exists_db() is not None:
        dump_db()
        drop_db()
    create_db()
    scheme_db()
    restore_db()
    restore_db()
    
    clear_and_print_header()
    print(tabulate(read_rows(table='record', limit=20), headers='keys', tablefmt='psql'))
    execute_frequency_job()

    operation = choose_operation()

    while True:
        execute_operation(operation)
        execute_frequency_job()
        dump_db()

        operation = choose_operation()

        clear_and_print_header()
        print(tabulate(read_rows(table='record', limit=20, order='title'), headers='keys', tablefmt='psql'))
    return False


if __name__ == "__main__":
    main()
