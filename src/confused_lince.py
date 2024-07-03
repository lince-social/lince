import pandas as pd
import psycopg2
from os.path import exists
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
    

def create_db_if_not_exists():
    connection = None
    try:
        connection = create_connection_object(database=None)
        cursor = connection.cursor()

        cursor.execute("SELECT datname FROM pg_database WHERE datname = 'lince'")

        result = cursor.fetchone()

        if result is not None:
            connection = create_connection_object()
            print("Database lince exists, dumping...")
            subprocess.run(['pg_dump', '-U' 'postgres', '-W', '-F', 'plain', '-f', 'db_versions/db_dump.sql', 'lince'], text=True, input='1\n')
            connection.close()

            connection = create_connection_object(database=None)
            print('Dump complete, dropping lince database')
            cursor.execute('DROP DATABASE lince')
            connection.close()

        connection = create_connection_object(database=None)
        cursor.execute('CREATE DATABASE lince')
        connection.close()

        connection = create_connection_object()
        sql_file_path = 'src/postgre.sql'

        if exists(sql_file_path):
            with open(sql_file_path, 'r') as file:
                sql_commands = file.read()

            cursor.execute(sql_commands)
            print(f"Database '{database_name}' populated successfully.")

        else:
            print(f"SQL file '{sql_file_path}' not found.")
   
    finally:
        if connection is not None:
            connection.close()


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


# def read_lines(table):
#     return (execute_query(f'SELECT * FROM {table}'))


# def add_line(table):
#     values = input('Add line: ')

#     execute_query(f'INSERT INTO {table} VALUES {values}')
#     return None


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


# def table_selection():
#      return int(input('''
# Select a Table:

# [0] Menu.
# [1] Cadastros.
# [*] Exit.

# Which table: '''))


# def operation_selection():
#     return int(input('''
# Select an operation:

# [0] Menu
# [1] Add
# [2] Delete
# [3] Uptade
# [*] Exit

# Which operation: '''))


def main():
    create_db_if_not_exists()
    # read_lines("cadastro").head()

    # table = table_selection()
    # read_lines(table).head()

    # match operation_selection():
    #     case 0:
    #         main()
    #     case 1:
    #         add_line(table)
    #     case 2:
    #         delete_line(table)
    #     case 3:
    #         update_line(table)
    #     case _:
    #         pass
    # main()


if __name__ == "__main__":
    main()
