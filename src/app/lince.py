import pandas as pd
import psycopg2
import os
import subprocess


def terminal_cleaner():
    # os.system('clear')
    return True


def lince_header_printer():
    terminal_cleaner()

    print('- Lince -')
    

def welcome_printer():
    lince_header_printer()

    print('\nWelcome to the Lince CLI.')


def create_connection_object(host = 'localhost', user = 'postgres', database = 'lince', password = '1', port = '5432'):
    connection = psycopg2.connect(
        host = host,
        user = user,
        database = database,
        password = password,
        port = port)

    connection.autocommit = True

    return connection


def exists_db():
    connection = create_connection_object(database=None)
    cursor = connection.cursor()

    cursor.execute("SELECT datname FROM pg_database WHERE datname = 'lince'")

    return cursor.fetchone()


def dump_db():
    print("\nDatabase lince exists, dumping...")

    connection = create_connection_object()
    cursor = connection.cursor()

    subprocess.run(['pg_dump', '-U' 'postgres', '-W', '-F', 'plain', '-f', '/home/eduardo/lince/src/db/versions/db_dump.sql', 'lince'], text=True, input='1\n')

    connection.close()

    return True


def drop_db():
    print('\nDump complete, dropping lince database...')

    connection = create_connection_object(database=None)
    cursor = connection.cursor()

    cursor.execute('DROP DATABASE lince')

    connection.close()

    return True


def create_db():
    print("\nCreating Database Lince...")

    connection = create_connection_object(database=None)
    cursor = connection.cursor()

    cursor.execute('CREATE DATABASE lince')

    connection.close()

    return True


def schema_db_chooser():
    match int(input("\nDatabase Schema Chooser\n\n[0] Lince Schema.\n[1] Other Schema.\n\nYour choice: ")):
        case 0:
            return '/home/eduardo/lince/src/db/postgre.sql'
        case 1:
            return input("Type the .sql file Path: ")

    return None


def fill_db():
    connection = create_connection_object()
    cursor = connection.cursor()

    sql_file_path = schema_db_chooser()

    if os.path.exists(sql_file_path):
        with open(sql_file_path, 'r') as file:
            sql_commands = file.read()

        cursor.execute(sql_commands)
        print(f"\nDatabase Lince populated successfully.")

    else:
        print(f"\nSQL file '{sql_file_path}' not found.")


def create_and_populate_db_if_not_already_created_or_populated():
    if exists_db() is not None:
        dump_db()
        drop_db()

    create_db()
    fill_db()

    return True
   

def app_mode():
    answer = int(input('''- Lince App -\n\n[0] Auto saver/dumper.\n[1] Save when called for.\n\nSelect mode: '''))

    if isinstance(answer, int) and answer in [0, 1]:
        return answer

    print("Wrong type, please type an Integer (int), i.e: 1, 42, 80")
    app_mode()


def read_lines(table, lines_number):
    return execute_query('SELECT * FROM {table} LIMIT {lines_number}')
    

def print_head_cadastro():
    terminal_cleaner()
    return read_lines(table='cadastro', lines_number=5)


def table_chooser():
    table = int(input('Select table:\n[0] Exit\n[1] Save\n[2] Cadastro'))

    if isinstance(table, int) and table in [0, 1, 2]:
        return table

    print('Wrong type or option selected, please enter an Integer (int) from 0 to 2')
    table_chooser()
    

def operation_options_printer():
    options_header = ['[S] Save.', '[E] Exit.'] 
    operation_options = ['[C] Create.','[R] Read.', '[U] Update.', '[D] Delete.']
    table_options = ['[1] Cadastro']

    

    max_element_size = max[(len(item) for item in options_header + operation_options + table_options)] 

    return None


def operation_chooser():
    operation_options_printer()
    choice = int(input('Your Choice: '))

    if isinstance(choice, int) and choice in [0, 1, 2]:
        return choice

    print('Wrong type or option, please enter an Integer from 0 to 2')
    operation_chooser()
    

def execute_operation():
    print_head_cadastro()

    table = table_chooser()
    operation = operation_chooser()

  
   
                    def add_line(table):
                        values = input('Add line: ')

                        execute_query(f'INSERT INTO {table} VALUES {values}')
                        retos.urn None


                    def delete_line(table):
                        where_clause = input('Where clause: ')

                        execute_query(f'DELETE FROM {table} WHERE {where_clause}')
                        return None


                    def update_line(table):
                        set_clause = input('Set clause: ')
                        where_clause = input('Where clause: ')

                        execute_query(f'UPDATE {table} SET {set_clause} WHERE {where_clause}')
                        execute_query(f'')
                        return None
    return True


def execute_query(query):
    connection = create_connection_object()
    connection.cursor().execute(query)    

    if query.startswith("SELECT"):
        df = pd.Dataframe(connection.cursor().fetchall(), columns=[desc[0] for desc in connection.cursor().description])
        connection.close()
        return df

    # elif query.startswith(("INSERT", "UPDATE", "DELETE")):
        # connection.commit()

    connection.commit()
    connection.close()
    return None


def main():
    welcome_printer()

    create_and_populate_db_if_not_already_created_or_populated()

    match app_mode():
        case 0:
            while True:
                lince_header_printer()
                print_head_cadastro()
                execute_operation()
                dump_db()
        case 1:
            while True:
                lince_header_printer()
                print_head_cadastro()
                execute_operation()
            dump_db

    return False


if __name__ == "__main__":
    main()
