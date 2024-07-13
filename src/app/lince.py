import pandas as pd
import psycopg2
import os
import subprocess

 
def create_connection_object(host = 'localhost', user = 'postgres', database = 'lince', password = '1', port = '5432'):
    connection = psycopg2.connect( host = host, user = user, database = database, password = password, port = port)
    connection.autocommit = True
    return connection




def execute_sql_command(command=None, database='lince'):
    connection = create_connection_object(database=database)
    cursor = connection.cursor()
    cursor.execute(command)    

    if command.startswith("SELECT"):
        df = pd.Dataframe(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])
        connection.close()
        return df

    return connection




def inform_db_exists_checking():
    return print('\nChecking if Lince Database exists...')
def check_exists_db():
    inform_db_exists_checking()

    connection = psycopg2.connect( host = 'localhost', user = 'postgres', password = '1', port = '5432')
    connection.autocommit = True
    cursor = connection.cursor()
    cursor.execute("SELECT datname FROM pg_database WHERE datname = 'lince'")
    return cursor.fetchone()




def inform_db_dumping():
    return print("\nDatabase lince exists, dumping...")
def dump_db():
    inform_db_dumping()
    return subprocess.run(['pg_dump', '-U' 'postgres', '-W', '-F', 'plain', '-f', '/home/eduardo/lince/src/db/versions/db_dump.sql', 'lince'], text=True, input='1\n')




def inform_db_dropping():
    return print('\nDump complete, dropping lince database...')
def drop_db(quiet=False):
    if quiet == False: inform_db_dumping()
    return execute_sql_command(command='DROP DATABASE lince', database=None)




def inform_db_creation():
    return print("\nCreating Database Lince...")
def create_db():
    inform_db_creation()
    connection = psycopg2.connect( host = 'localhost', user = 'postgres', password = '1', port = '5432')
    connection.autocommit = True
    cursor = connection.cursor()

    cursor.execute('CREATE DATABASE lince')

    # execute_sql_command(command='CREATE DATABASE lince', database=None)
    return True




def inform_db_filling():
    return('\nLince Database created. Starting filling process...')
def print_db_schema_options():
    inform_db_filling()
    return print("\nDatabase Schema Chooser\n\n[0] Lince Schema.\n[1] Other Schema.")
def choose_db_schema():
    print_db_schema_options()

    match int(input("\n\nYour choice: ")):
        case 0:
            return '/home/eduardo/lince/src/db/postgre.sql'
        case 1:
            sql_file_path = input("Type the .sql file Path: ")
            if exists(sql_file_path): return sql_file_path

            print(f"\nSQL file '{sql_file_path}' not found.")

    choose_db_schema()
def fill_db():
    with open(choose_db_schema(), 'r') as file: return execute_sql_command(command = file.read())




def create_and_populate_db_if_not_already_created_or_populated():
    if check_exists_db() is not None:
        dump_db()
        drop_db()
    create_db()
    return fill_db()




def clean_terminal():
    return os.system('clear')


def print_lince_header():
    return print('- Lince -')
    

def choose_app_mode():
    answer = int(input('''- Lince App -\n\n[0] Autosaver.\n[1] Save when called for.\n\nSelect mode: '''))

    if isinstance(answer, int) and answer in [0, 1]: return answer

    print("Wrong type, please type an Integer (int), i.e: 1, 42, 80")
    choose_app_mode()
def print_app_mode(app_mode):
    return print('Autosaver ON') if app_mode == 0 else print('Autosaver OFF')




def read_lines(table, rows):
    return execute_sql_command('SELECT * FROM {table} LIMIT {lines_number}')




def print_cadastro_rows(number):
    return read_lines(table='cadastro', rows=number)




def print_top_info(app_mode=None):
    clean_terminal()
    print_lince_header()
    print_app_mode(app_mode)
    print_cadastro_rows(5)




def print_operation_options():
    options_header = ['[E] Exit.', '[S] Save.'] 
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


def choose_operation():
    print_operation_options()
    choice = int(input('Your Choice: '))

    if isinstance(choice, int) and choice in [0, 1, 2]: return choice 

    print('Wrong type or option, please enter an Integer from 0 to 2')
    choose_operation()
    

def execute_operation(operation):

    
                    # def add_line(table):
                    #     values = input('Add line: ')

                    #     execute_sql_command(f'INSERT INTO {table} VALUES {values}')
                    #     retos.urn None


                    # def delete_line(table):
                    #     where_clause = input('Where clause: ')

                    #     execute_sql_command(f'DELETE FROM {table} WHERE {where_clause}')
                    #     return None


                    # def update_line(table):
                    #     set_clause = input('Set clause: ')
                    #     where_clause = input('Where clause: ')

                    #     execute_sql_command(f'UPDATE {table} SET {set_clause} WHERE {where_clause}')
                    #     execute_sql_command(f'')
                    #     return None
    return True


    

def main():
    create_and_populate_db_if_not_already_created_or_populated()

    app_mode = choose_app_mode()

    print_top_info(app_mode)

    operation = choose_operation()

    while True:
        execute_operation(operation)
        if app_mode == 0: dump_db(quiet=True) 
        operation = choose_operation()
        print_top_info(app_mode)

    dump_db()
    return False


if __name__ == "__main__":
    main()
