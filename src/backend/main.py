from dateutil.relativedelta import relativedelta
import subprocess
import json
import sys
import os
import re
from datetime import datetime, timedelta
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

    if command.strip().upper().startswith("SELECT"):
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
    return subprocess.run(['pg_dump', '--data-only','--inserts', '--no-owner', '--no-privileges', '-U', 'postgres', '--no-password', '-F', 'plain', '-f', f'{os.path.abspath(os.path.join(__file__,'..','..',  "db", "dump.sql"))}', 'lince'], text=True, input='1\n')

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
    with open(os.path.abspath(os.path.join(__file__,'..','..',  "db", "postgre.sql")), 'r') as file: return execute_sql_command(command = file.read())

# def restore_db():
#     p = subprocess.Popen(f"psql -h 'localhost' -d 'lince' -U postgres < {os.path.abspath(os.path.join(__file__,'..','..','..', "src", "db", "dump.sql"))}", shell=True, stdin=subprocess.PIPE)
#     return p.communicate(b"1\n")

def restore_db():
    command = f"psql -h 'localhost' -d 'lince' -U postgres < {os.path.abspath(os.path.join(__file__, '..', '..', '..', 'src', 'db', 'dump.sql'))}"
    p = subprocess.Popen(command, shell=True, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL)
    return p.communicate(b"1\n")

def print_help():
    with open(os.path.abspath(os.path.join(__file__,'..','..','..',  "README")), 'r') as file:
        print(file.read())
        return input('(Press any button to continue)')


def create_row(table):
    tablecolumns = execute_sql_command(command=f"SELECT * FROM {table} WHERE false")

    columns = "("
    row = "("

    n = 0

    for column in tablecolumns:
        if column == 'id':
            continue

        print()
        print()
        print(return_column_information(column), end='')
        print()
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


def truncate_column(column, truncation_size):
    if column == None:
        return column

    lines = []

    while len(column) > truncation_size:
        lines.append(column[:truncation_size])
        column = column[truncation_size:]

    lines.append(column)
    return '\n'.join(lines)

def read_rows(command):
    configuration_df = execute_sql_command('select truncation from configuration order by quantity DESC limit 1')

    for index, configuration_row in configuration_df.iterrows():
        truncation = configuration_row['truncation']

    if isinstance(truncation, str):
        truncation = json.loads(configuration_row['truncation'])


    rows = execute_sql_command(command=command)

    if not isinstance(rows, pd.DataFrame): return None

    for column in rows.columns:
        if column in truncation:
            truncation_size = truncation[column]
            rows[column] = rows[column].apply(lambda x: truncate_column(x, truncation_size))

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


def return_column_information(column):
    configuration_df = read_rows('select * from configuration')
    max_quantity_config = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]
    column_information_mode = max_quantity_config['column_information_mode']

    if column_information_mode != 'silent':
        info = 'Column: '

    if column_information_mode == 'verbose':
        match column:
            case "quantity":
                info += '"quantity REAL NOT NULL DEFAULT 1". Responsible for quantifying the availability of the phenomenon. It saves the information of how much. Positive numbers make it run or available. Negative numbers make it a need, in the case of frequency it will run untill it turns to 0. If zero, it is as good as not existing.'
            case "save_mode":
                info += '"save_mode VARCHAR(9) NOT NULL DEFAULT "Automatic" CHECK (save_mode in ("Automatic", "Manual"))". Responsible for determining the save mode, either Automatic or Manual. After each operation the system can save or let the database be saved when s or S is typed on the menu.'
            case "view":
                info += '"view TEXT NOT NULL DEFAULT "SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, title ASC, description ASC". Responsible for configuring what tables will be shown on the main page..'
            case "column_information_mode":
                info += '"column_information_mode VARCHAR(7) NOT NULL DEFAULT "verbose" CHECK (column_information_mode in ("verbose", "short", "silent"))". Responsible for determining the amount of information shown after every column is queried, so the user can understand the details and restrictions of it.'
            case "keymap":
                info += '"keymap jsonb NOT NULL DEFAULT ""{}". Responsible for storing the keymap configuration, for personalized operations.'
            case "truncation":
                info += '"truncation jsonb NOT NULL DEFAULT "{"record": {"description": 150}}". Responsible for defining the truncation for each column. When a table is being printed, it will follow the instructions in this configuration, so every so and so characters (i.e. 50) a newline is added to occupy space vertically.'
            case "title":
                info += '"title VARCHAR(50) NOT NULL". Responsible for storing the title of the record. It is one of the possible ways to search and identify a record..'
            case "description":
                info += '"description TEXT". Responsible for storing the description of the record.'
            case "location":
                info += '"location VARCHAR(255)". Responsible for storing the location of the record.'
            case "day_week":
                info += '"day_week INTEGER". Responsible for storing the day of the week for the frequency.'
            case "months":
                info += '"months REAL DEFAULT 0 NOT NULL". Responsible for storing the months component of the frequency.'
            case "days":
                info += '"days REAL DEFAULT 0 NOT NULL". Responsible for storing the days component of the frequency.'
            case "seconds":
                info += '"seconds REAL DEFAULT 0 NOT NULL". Responsible for storing the seconds component of the frequency.'
            case "next_date":
                info += '"next_date TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL". Responsible for storing the next date of the frequency.'
            case "record_id":
                info += '"record_id INTEGER REFERENCES record(id) ON DELETE CASCADE NOT NULL". Responsible for linking the frequency to the corresponding record.'
            case "delta":
                info += '"delta REAL DEFAULT 0 NOT NULL". Responsible for storing the delta value of the frequency.'
            case "finish_date":
                info += '"finish_date DATE". Responsible for storing the finish date of the frequency.'
            case "when_done":
                info += '"when_done BOOLEAN DEFAULT false". Responsible for storing the status of the frequency (done or not).'
    elif column_information_mode == "short":
        match column:
            case "quantity":
                info += '"quantity REAL NOT NULL DEFAULT 1"'
            case "save_mode":
                info += '"save_mode VARCHAR(9) NOT NULL DEFAULT "Automatic" CHECK (save_mode in ("Automatic", "Manual"))".'
            case "view":
                info += '"view TEXT NOT NULL DEFAULT "SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, title ASC, description ASC".'
            case "column_information_mode":
                info += '"column_information_mode VARCHAR(7) NOT NULL DEFAULT "verbose" CHECK (column_information_mode in ("verbose", "short", "silent"))".'
            case "keymap":
                info += '"keymap jsonb NOT NULL DEFAULT ""{}".'
            case "truncation":
                info += '"truncation jsonb NOT NULL DEFAULT ""{"record": {"description": 150}}".'
            case "title":
                info += '"title VARCHAR(50) NOT NULL".'
            case "description":
                info += '"description TEXT".'
            case "location":
                info += '"location VARCHAR(255)".'
            case "day_week":
                info += '"day_week INTEGER"'
            case "months":
                info += '"months REAL DEFAULT 0 NOT NULL"'
            case "days":
                info += '"days REAL DEFAULT 0 NOT NULL"'
            case "seconds":
                info += '"seconds REAL DEFAULT 0 NOT NULL"'
            case "next_date":
                info += '"next_date TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL"'
            case "record_id":
                info += '"record_id INTEGER REFERENCES record(id) ON DELETE CASCADE NOT NULL"'
            case "delta":
                info += '"delta REAL DEFAULT 0 NOT NULL"'
            case "finish_date":
                info += '"finish_date DATE"'
            case "when_done":
                info += '"when_done BOOLEAN DEFAULT false"'

    return info

def bring_consequences():
    record_df = read_rows('SELECT * FROM record')

    frequency_df = read_rows('select * from frequency')
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
        elif frequency_row['quantity'] == 0:
            continue
        elif frequency_row['quantity'] < 0:
            quantity = frequency_row['quantity'] + 1
            update_rows('frequency', set_clause=f"quantity = {quantity}", where_clause=f"id = {frequency_row['id']}")

        next_date += relativedelta(months=frequency_row['months']) + timedelta(days=frequency_row['days'], seconds=frequency_row['seconds'])

        if today == next_date:
            next_date += timedelta(days=1)

        if pd.isna(frequency_row['day_week']):
            pass
        else:
            while next_date.isoweekday() not in [int(i) for i in str(int(frequency_row['day_week']))]:
                next_date += timedelta(days=1)


def karma():
    karma_df = read_rows('SELECT * FROM karma')

    for index, karma_row in karma_df.iterrows():
        karma = karma_row['karma']
        karma = [item.strip() for item in karma.split('=')]

        karma_one_record_quantities = re.findall('rq[0-9]+', karma[1])

        for id in karma_one_record_quantities:
            try:
                quantity = execute_sql_command(f"SELECT quantity FROM record WHERE id = {id[2:]}")['quantity'].iloc[0]
                karma[1] = karma[1].replace(id, str(quantity))
            except Exception as e:
                print(e)

        karma_one_frequencies = re.findall('f[0-9]+', karma[1])

        for frequency in karma_one_frequencies:
            frequency_return = check_update_frequency(id=frequency[1:])
            karma[1] = karma[1].replace(frequency, str(frequency_return))

        # rodar todos os scripts e inserir o retorno deles
        karma_one_commands = re.findall('c[0-9]+', karma[1])
        # print(karma_one_commands)

        print(karma)
        
        try:
            result = eval(karma[1])
        except:
            print(f'There was an error, check karma(id) = {karma_row['id']}')
            continue

        if result != 0:
            if 'c' in karma[0]:
                print(karma[0][1:])
                execute_shell_command(karma[0][1])
                continue

            if 'rq' in karma[0]:
                table = 'record'
                set_column = 'quantity'
                set_value = result
                where_column = 'id'
                where_value = f'{karma[0][2:]}'
                execute_sql_command(f'UPDATE {table} SET {set_column} = {set_value} WHERE {where_column} = {where_value}')
              # update_rows('record', set_clause=f'{set_column} = {set_value}', where_clause=f'{where_column} = {where_value}')
    return True


def execute_shell_command(id):
    command_row = read_rows(f'SELECT * FROM command WHERE id={id}')
    command_row = command_row.iloc[0]

    quantity = command_row['quantity']

    if quantity == 0: return 0
    if quantity < 0: update_rows('command', set_clause=f"quantity = {quantity + 1}", where_clause=f"id = {command_row['id']}")

    # return os.system(command_row['command'])
    result = subprocess.run(command_row['command'].split(), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    try:
        print(result.stderr)
        return result.stdout
    except Exception as e:
        print(e)

    return False


def execute_operation(operation):
    if operation.isdigit():
        return execute_sql_command(command=f'UPDATE record SET quantity = 0 WHERE ID = {operation}')
    elif '0' in operation:
        table = 'configuration'
    elif '1' in operation:
        table = 'history'
    elif '2' in operation:
        table = 'record'
    elif '3' in operation:
        table = 'karma'
    elif '4' in operation:
        table = 'frequency'
    elif '5' in operation:
        table = 'command'
    elif '6' in operation:
        table = 'transfer'
    else:
        table = 'record'

    if 'c' in operation or 'C' in operation:
        create_row(table)
    elif 'r' in operation or 'R' in operation:
        return read_rows(f'select * from {table}')
    elif 'u' in operation or 'U' in operation:
        update_rows(table)
    elif 'd' in operation or 'D' in operation:
        delete_rows(table)
    elif 'f' in operation or 'F' in operation:
        execute_sql_command_from_file()
    elif 'e' in operation or 'E' in operation:
        sys.exit()
    elif 's' in operation or 'S' in operation:
        dump_db()
    elif 'l' in operation or 'L' in operation:
        restore_db()
    elif 'h' in operation or 'H' in operation:
        print_help()
    elif 'q' in operation or 'Q' in operation:
        return execute_sql_command(command=input('Type the SQL command: '))

    return True
