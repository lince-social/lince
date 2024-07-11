import pandas as pd
import psycopg2
import os
import subprocess


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
    print("Database lince exists, dumping...")

    connection = create_connection_object()

    subprocess.run(['pg_dump', '-U' 'postgres', '-W', '-F', 'plain', '-f', '/home/eduardo/lince/src/db/versions/db_dump.sql', 'lince'], text=True, input='1\n')

    connection.close()

    return True


def drop_db():
    print('Dump complete, dropping lince database')

    connection = create_connection_object(database=None)
    cursor = connection.cursor()

    cursor.execute('DROP DATABASE lince')

    connection.close()

    return True


def create_db():
    print("Creating Database Lince")

    connection = create_connection_object(database=None)

    cursor.execute('CREATE DATABASE lince')

    connection.close()

    return True


def fill_db_file_path():
    match int(input("[0] Dumped DB.\n[1] Specific .sql file.\nChoose a fill method: ")):
        case 0:
            return '/home/eduardo/lince/src/db/postgre.sql'
        case 1:
            return input("Type the .sql file Path: ")

    return None


def fill_db():
    sql_file_path = choose_fill_db_method()

    if os.path.exists(sql_file_path):
        with open(sql_file_path, 'r') as file:
            sql_commands = file.read()

        cursor.execute(sql_commands)
        print(f"Database '{database_name}' populated successfully.")

    else:
        print(f"SQL file '{sql_file_path}' not found.")


def create_and_populate_db_if_not_already_created_or_populated():
    if exists_db() is not None:
        dump_db()
        drop_db()

    create_db()
    fill_db()

    return True
   

def app_mode():
    answer = int(input('''Select mode:\n[0] Auto saver/dumper\n[1]Save when called for'''))

    if isinstance(answer, int) and answer in [0, 1]:
        return answer

    print("Wrong type, please type an Integer (int), i.e: 1, 42, 80")
    app_mode()


def read_lines(table, lines_number):
    return execute_query('SELECT * FROM {table} LIMIT {lines_number}')
    

def print_head_cadastro():
    return read_lines(table='cadastro', lines_number=5)


def table_chooser():
    table = int(input('Select table:\n[0] Exit\n[1] Save\n[2] Cadastro'))

    if isinstance(table, int) and table in [0, 1, 2]:
        return table

    print('Wrong type or option selected, please enter an Integer (int) from 0 to 2')
    table_chooser()
    

def operation_chooser():
    choice = int(input('Select table:\n[0] Exit.\n[1] Save.\n[2] Select.'))

    if isinstance(choice, int) and choice in [0, 1, 2]:
        return choice

    print('Wrong type or option, please enter an Integer from 0 to 2')
    operation_chooser()
    

def execute_operation():
    print_head_cadastro()

    table = table_chooser()
    operation = operation_chooser()


    return True


def main():
    match app_mode():
        case 0:
            while True:
                create_and_populate_db_if_not_already_created_or_populated()
                execute_operation()
        case 1:
            create_and_populate_db_if_not_already_created_or_populated()
            while True:
                execute_operation()

    return False


# def execute_query(query):
#     connection = create_connection_object()
#     connection.cursor().execute(query)    

#     if query.startswith("SELECT"):
#         connection.close()
#         return pd.Dataframe(connection.cursor().fetchall(), columns=[desc[0] for desc in connection.cursor().description])
#     elif query.startswith(("INSERT", "UPDATE", "DELETE")):
#         connection.commit()
#     connection.close()
#     return None


# def add_line(table):
#     values = input('Add line: ')

#     execute_query(f'INSERT INTO {table} VALUES {values}')
#     retos.urn None


# def delete_line(table):
#     where_clause = input('Where clause: ')

#     execute_query(f'DELETE FROM {table} WHERE {where_clause}')
#     return None


# def update_line(table):
#     set_clause = input('Set clause: ')
#     where_clause = input('Where clause: ')

#     execute_query(f'UPDATE {table} SET {set_clause} WHERE {where_clause}')
#     execute_query(f'')
#     return None


if __name__ == "__main__":
    main()
