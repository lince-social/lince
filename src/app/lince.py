import pandas as pd
import sys
import psycopg2
import os
import subprocess


def create_connection_object(host = 'localhost', user = 'postgres', database = 'lince', password = '1', port = '5432'):
    connection = psycopg2.connect( host = host, user = user, database = database, password = password, port = port)
    connection.autocommit = True
    return connection


def inform_db_exists_checking():
    return print('\nChecking if Lince Database exists...')
def check_exists_db():
    connection = psycopg2.connect( host = 'localhost', user = 'postgres', password = '1', port = '5432')
    connection.autocommit = True
    cursor = connection.cursor()
    cursor.execute("SELECT datname FROM pg_database WHERE datname = 'lince'")
    result = cursor.fetchone()
    connection.close()
    return result
def inform_db_dumping():
    return print("\nDatabase lince exists, dumping...")
def dump_db():
    return subprocess.run(['pg_dump', '-U' 'postgres', '-W', '-F', 'plain', '-f', '/home/eduardo/lince/src/db/versions/db_dump.sql', 'lince'], text=True, input='1\n')
def inform_db_dropping():
    return print('\nDump complete, dropping lince database...')
def drop_db(quiet=False):
    return execute_sql_command(command='DROP DATABASE lince', database=None)
def inform_db_creation():
    return print("\nCreating Database Lince...")
def create_db():
    connection = psycopg2.connect( host = 'localhost', user = 'postgres', password = '1', port = '5432')
    connection.autocommit = True
    cursor = connection.cursor()
    cursor.execute('CREATE DATABASE lince')
    connection.close()
    return True
def inform_db_filling():
    return('\nLince Database created. Starting filling process...')
def fill_db():
    # with open(choose_db_schema(), 'r') as file: return execute_sql_command(command = file.read())
    with open(os.path.abspath(os.path.join(__file__,"..", "..", "..", "src", "db", "postgre.sql")), 'r') as file: return execute_sql_command(command = file.read())


def choose_app_mode():
    answer = int(input('''Please select a saving mode from the options below:\n\n[0] Autosave ON  (Will save your data through pg_dump after every operation).\n[1] Autosave OFF (Only save when ordered by the user).\n\nSelect mode: '''))

    return answer if isinstance(answer, int) and answer in [0, 1] else choose_app_mode()


def clean_terminal():
    return os.system('clear')

def print_lince_header():
    return print('- Lince -')

def print_app_mode(app_mode):
    return print('Autosaver ON') if app_mode == 0 else print('Autosaver OFF')


def execute_sql_command(command=None, database='lince'):
    connection = create_connection_object(database=database)
    cursor = connection.cursor()
    cursor.execute(command)    
    if command.startswith("SELECT"):
        df = pd.DataFrame(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])
        connection.close()
        return df
    return connection

def read_rows(table, rows = 0):
    if rows <= 0: rows = int(input(f'How many rows to fetch from "{table}"? '))
    return execute_sql_command(f'SELECT * FROM {table} LIMIT {rows}')

   
def create_row(table):
    return execute_sql_command(f'INSERT INTO {table} VALUES {input("Add line: ")}')

def delete_row(table):
    return execute_sql_command(f'DELETE FROM {table} WHERE {input("Where clause: ")}')

def update_row(table):
    return execute_sql_command(f'UPDATE {table} SET {input("Set clause: ")} WHERE {input("Where clause: ")}')


def print_operation_options():
    options_header = ['[E] Exit.', '[S] Save.', '[I] Insert .sql File'] 
    operation_options = ['[C] Create.','[R] Read.', '[U] Update.', '[D] Delete.' ]
    table_options = ['[1] Cadastro']

    max_length = max(len(item) for item in (options_header + operation_options + table_options))
    max_len_list = max(len(operation_options), len(options_header), len(table_options))

    options_header += [''] * (max_len_list - len(options_header))
    operation_options += [''] * (max_len_list - len(operation_options))
    table_options += [''] * (max_len_list - len(table_options))

    print('-' * (3 * max_length + 10))

    for h, o, t in zip(options_header, operation_options, table_options):
        print(f"| {h:{max_length}} | {o:{max_length}} | {t:{max_length}} |")

    print('-' * (3 * max_length + 10))
    return True

def collect_operation_chosen():
    return input('Your choice: ')

def execute_operation(operation):
    if 'e' or 'E' in operation:
        sys.exit()
    if 's' or 'S' in operation:
        dump_db()

    if '1' in operation:
        table = 'cadastro'

    if 'c' or 'C' in operation:
        add_line(table)
    if 'r' or 'R' in operation:
        read_rows(table)
    if 'u' or 'U' in operation:
        update_rows(table)
    if 'd' or 'D' in operation:
        delete_rows(table)


def main():
    inform_db_exists_checking()
    if check_exists_db() is not None:
        inform_db_dumping(); dump_db()
        inform_db_dropping(); drop_db()
    inform_db_creation(); create_db()
    inform_db_filling(); fill_db()

    clean_terminal()
    print_lince_header()
    read_rows(table='cadastro', rows=5)

    print_operation_options()
    operation = collect_operation_chosen()

    while True:
        execute_operation(operation)
        print_operation_options()
        operation = collect_operation_chosen()

        clean_terminal()
        print_lince_header()
        read_rows(table='cadastro', rows=5)
    return False


if __name__ == "__main__":
    main()
