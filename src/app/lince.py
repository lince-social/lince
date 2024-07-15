import os
import sys
import subprocess
import psycopg2
import pandas as pd


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
        return df.to_string(index=False)

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
    return subprocess.run(['pg_dump', '-U' 'postgres', '-W', '-F', 'plain', '-f', 'src/db/versions/db_dump.sql', 'lince'], text=True, input='1\n')

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


def clear_and_print_header():
    os.system('clear')
    print('- Lince -')
    return print()


def choose_operation():
    options_header = [
        '[E] Exit',
        '[S] Save DB',
        '[Q] Specific Query',
        '[L] Load DB',
        '[I] Insert .sql File'
    ]
    max_len_options = max(len(item) for item in (options_header))

    operation_options = [
        '[C] Create',
        '[R] Read',
        '[U] Update',
        '[D] Delete'
    ]
    max_len_operations = max(len(item) for item in (operation_options))

    table_options = [
        '[1] Record',
        '[2] Transfer',
        '[3] Frequency',
        '[4] Checkpoint',
        '[5] Delta',
        '[6] Rate',
        '[7] Proportion',
        '[8] Shell Command',
    ]
    max_len_table = max(len(item) for item in (table_options))

    max_len_list = max(len(operation_options), len(options_header), len(table_options))

    options_header += [''] * (max_len_list - len(options_header))
    operation_options += [''] * (max_len_list - len(operation_options))
    table_options += [''] * (max_len_list - len(table_options))

    print('-' * (max_len_options + max_len_operations + max_len_table + 10))

    for h, o, t in zip(options_header, operation_options, table_options):
        print(f"| {h:{max_len_options}} | {o:{max_len_operations }} | {t:{max_len_table }} |")

    print('-' * (max_len_options + max_len_operations + max_len_table + 10))

    return input('Your choice: ')


def execute_operation(operation):
    if ('e' or 'E') in operation:
        sys.exit()
    if ('s' or 'S') in operation:
        dump_db()
    if ('q' or 'Q') in operation:
        execute_sql_command(command=input('SQL command: '))
    if ('l' or 'L') in operation:
        restore_db()
    if ('i' or 'I') in operation:
        insert_file()

    if '1' in operation:
        table = 'record'
    if '2' in operation:
        table = 'frequency'

    if ('c' or 'C') in operation:
        create_row(table)
    if ('r' or 'R') in operation:
        read_rows(table)
    if ('u' or 'U') in operation:
        update_rows(table)
    if ('d' or 'D') in operation:
        delete_rows(table)


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


def read_rows(table, rows = 0):
    if rows <= 0:
        rows = input(f'Number of rows to fetch from {table}: ')
        if not isinstance(rows, int): rows = 10

    print(execute_sql_command(command=f'SELECT * FROM {table} LIMIT {rows}'))

    return print()


def update_rows(table):
    return execute_sql_command(command=f'UPDATE {table} SET {input("Set clause: ")} WHERE {input("Where clause: ")}')


def delete_rows(table):
    return execute_sql_command(command=f'DELETE FROM {table} WHERE {input("Where clause: ")}')

def main():
    if check_exists_db() is not None:
        dump_db()
        drop_db()
    create_db()
    scheme_db()
    restore_db()

    clear_and_print_header()
    read_rows(table='record', rows=10)

    operation = choose_operation()

    while True:
        execute_operation(operation)
        operation = choose_operation()

        clear_and_print_header()
        read_rows(table='record', rows=10)
    return False


if __name__ == "__main__":
    main()
