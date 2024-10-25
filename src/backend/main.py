from dateutil.relativedelta import relativedelta
import subprocess
import json
import sys
import os
import re
from datetime import datetime, timedelta, timezone
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
    db='default'
    return subprocess.run(['pg_dump', '--data-only', '--inserts', '--no-owner', '--no-privileges', '-U', 'postgres', '--no-password', '-F', 'plain', '-f', f'{os.path.abspath(os.path.join(__file__, '..', '..', "db","versions", f"{db}.sql"))}', 'lince', '-h', 'localhost', '-p', '5432'], text=True, input='1\n')

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
    with open(os.path.abspath(os.path.join(__file__,'..','..',  "db", "schema.sql")), 'r') as file: return execute_sql_command(command = file.read())

def restore_db():
    try:
        db_path = os.path.abspath(os.path.join(__file__, '..', '..', 'db','versions', 'default.sql'))
        command = f"psql -h 'localhost' -d 'lince' -U postgres < {db_path}"
        p = subprocess.Popen(command, shell=True, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL)
        return p.communicate(b"1\n")
    except Exception as e:
        print(e)

def insert_ifnot_db():
    with open(os.path.abspath(os.path.join(__file__,'..','..',  "db", "insert_ifnot.sql")), 'r') as file: return execute_sql_command(command = file.read())

def print_help():
    with open(os.path.abspath(os.path.join(__file__,'..','..','..',  "README")), 'r') as file:
        print(file.read())
        return input('(Press any button to continue)')


def return_column_information(column):
    configuration_df = read_rows('select * from configuration')
    max_quantity_config = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]
    column_information_mode = max_quantity_config['column_information_mode']

    info = ''

    if column_information_mode == 'verbose':
        match column:
            case "quantity":
                info += '"quantity REAL NOT NULL DEFAULT 1". Responsible for quantifying the availability of the phenomenon. It saves the information of how much. Positive numbers make it run or available. Negative numbers make it a need, in the case of frequency it will run untill it turns to 0. If zero, it is as good as not existing.'
            case "text":
                info += '"text TEXT". Responsible for.'
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
                info += '"record_id INTEGER". Responsible for linking with the corresponding record.'
            case "delta":
                info += '"delta REAL DEFAULT 0 NOT NULL". Responsible for storing the delta value of the frequency.'
            case "finish_date":
                info += '"finish_date DATE". Responsible for storing the finish date of the frequency.'
            case "when_done":
                info += '"when_done BOOLEAN DEFAULT false". Responsible for storing the status of the frequency (done or not).'
            case "interval_mode":
                info +='interval_mode VARCHAR(10) NOT NULL  DEFAULT "fixed" CHECK (interval_mode IN ("fixed", "relative"))'
            case "sum_mode":
                info += 'sum_mode INTEGER NOT NULL DEFAULT 0 CHECK (sum_mode in (-1,0,1))'
            case "end_date":
                info += 'end_date TIMESTAMP WITH TIME ZONE DEFAULT now()'
            case "interval_lenght":
                info += 'interval_length INTERVAL NOT NULL'
            case "end_lag":
                info += 'end_lag interval NOT NULL'

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

def create_row(table):
    tablecolumns = execute_sql_command(command=f"SELECT * FROM {table} WHERE false")

    columns = "("
    row = "("

    n = 0

    for column in tablecolumns:
        if column == 'id':
            continue

        print()
        print(return_column_information(column))
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

def read_rows(command, where_id_in=None, view_mode=None):
    configuration_row = execute_sql_command('select truncation, table_query from configuration order by quantity DESC limit 1').iloc[0]

    table_query = configuration_row['table_query']
    truncation = configuration_row['truncation']

    if where_id_in != None: command += where_id_in
    else:
        for key, value in table_query.items():
            if command == f'SELECT * FROM {key}':
                command = value

    rows = execute_sql_command(command=command)

    if not isinstance(rows, pd.DataFrame): return None

    if view_mode != None:
        for column in rows.columns:
            if column in truncation:
                truncation_size = truncation[column]
                rows[column] = rows[column].apply(lambda x: truncate_column(x, truncation_size))

    return rows

def update_rows(table, set_clause=None, where_clause=None, where_id_in=None):
    if set_clause == None:
        print(f'UPDATE {table}')
        set_clause = input("SET ")
    if set_clause != "": command=f'UPDATE {table} SET {set_clause}'

    if where_id_in != None: command += where_id_in

    if where_clause == None:
        where_clause = input("WHERE ")
    if where_clause != "": command += f' WHERE {where_clause}'

    return execute_sql_command(command=command)


def delete_rows(table, where_clause=None, where_id_in=None):
    command = f'DELETE FROM {table}'
    print(command, end='')

    if where_clause == None:
        print('no WHERE CAUSE deletes all')
        where_clause = input("WHERE ")

    if where_id_in != None:
        command += where_id_in
        if where_clause != "":
            command += f' AND {where_clause}'
    elif where_clause != "":
        command += f' WHERE {where_clause}'

    return execute_sql_command(command=command)


def activate_configuration(id):
    return execute_sql_command(f'UPDATE configuration SET quantity = CASE WHEN id = {id} THEN 1 ELSE 0 END')


def execute_sql_command_from_file():
    file_path = os.path.realpath(os.path.join(__file__, '..', '..', '..', input('File path starting from the lince dir: ')))

    with open(file_path, 'r') as file:
        return execute_sql_command(command = file.read())


def check_update_frequency(id):
    frequency_row = read_rows(f'SELECT * FROM frequency WHERE id={id} and quantity != 0')
    if frequency_row.empty:
        print(f'No such frequency row with id {id}')
        return 0
    else:
        frequency_row = frequency_row.iloc[0]

    configuration_row = execute_sql_command('SELECT timezone FROM configuration ORDER BY quantity DESC LIMIT 1').iloc[0]
    configuration_timezone = configuration_row['timezone']
    # print("timezone: " + configuration_timezone)

    tz_offset = timedelta(hours=int(configuration_timezone))
    # print(f'tzoffset: {tz_offset}')
    tzinfo = timezone(tz_offset)
    # print(f"tzinfo: {tzinfo}")

    next_date = frequency_row['next_date'].astimezone(tzinfo)
    # next_date = frequency_row['next_date']
    time_now = datetime.now(tzinfo)

    # print(f"1: {next_date} 2: {time_now}")
    if frequency_row['finish_date'] is not None and time_now.date() > frequency_row['finish_date']:
        return 0

    if next_date > time_now or frequency_row['quantity'] == 0:
        return 0

    if frequency_row['quantity'] < 0:
        quantity = frequency_row['quantity'] + 1
        update_rows('frequency', set_clause=f"quantity = {quantity}", where_clause=f"id = {frequency_row['id']}")

    # print(f": {}")
    next_date += relativedelta(months=int(frequency_row['months'])) + timedelta(days=int(frequency_row['days']), seconds=int(frequency_row['seconds']))

    if not pd.isna(frequency_row['day_week']):
        next_date += timedelta(days=1)
        while next_date.isoweekday() not in [int(i) for i in str(int(frequency_row['day_week']))]:
            next_date += timedelta(days=1)

    update_rows('frequency', set_clause=f"next_date = '{next_date}'", where_clause=f'id = {frequency_row["id"]}')

    return True


def execute_shell_command(id):
    command_row = read_rows(f'SELECT * FROM command WHERE id={id}')
    if command_row.empty: return False

    command_row = command_row.iloc[0]
    quantity = command_row['quantity']

    if quantity == 0: return 0
    if quantity < 0: update_rows('command', set_clause=f"quantity = {quantity + 1}", where_clause=f"id = {command_row['id']}")

    # result = subprocess.Popen(command_row['command'].split(), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    os.system(command_row['command'])

    # try: return result.stdout
    # except Exception as e: print(e)

    return False


def karma():
    karma_df = read_rows('SELECT * FROM karma')

    for index, karma_row in karma_df.iterrows():
        expression = karma_row['expression']
        expression = [item.strip() for item in expression.split('=', 1)]

        try:
            expression_one_record_quantities = re.findall('rq[0-9]+', expression[1])
            for id in expression_one_record_quantities:
                quantity = execute_sql_command(f"SELECT quantity FROM record WHERE id = {id[2:]}")['quantity'].iloc[0]
                expression[1] = expression[1].replace(id, str(quantity))

            expression_one_frequencies = re.findall('f[0-9]+', expression[1])
            for frequency in expression_one_frequencies:
                frequency_return = check_update_frequency(id=frequency[1:])
                expression[1] = expression[1].replace(frequency, str(frequency_return))

            expression_one_commands = re.findall('c[0-9]+', expression[1])
            for command in expression_one_commands:
                command_return = execute_shell_command(id=command[1:])
                expression[1] = expression[1].replace(command, str(command_return))

            expression_one_sums = re.findall('s[0-9]+', expression[1])
            for sum in expression_one_sums:
                sum_return = return_sum_delta_record(id=sum[1:])
                expression[1] = expression[1].replace(sum, str(sum_return))

            result = eval(expression[1])
            if result == None:
                continue
        except:
            continue

        if result != 0 or (result == 0 and expression[0].endswith('*')):
            expression[0] = expression[0].replace('*', '')
            expression[0] = expression[0].strip()
            left_expression = expression[0].split(',')

            for consequence in left_expression:
                consequence = consequence.strip()

                if 'rq' in consequence:
                    table = 'record'
                    set_column = 'quantity'
                    set_value = result
                    where_column = 'id'
                    where_value = f'{consequence[2:]}'
                    execute_sql_command(f'UPDATE {table} SET {set_column} = {set_value} WHERE {where_column} = {where_value}')
                    dump_db()

                if 'c' in consequence:
                    execute_shell_command(consequence[1:])
                    continue

    return True


def return_sum_delta_record(id):
    sum_row = read_rows(f'SELECT * FROM sum WHERE id={id} AND quantity != 0')
    
    if sum_row.empty:
        print(f'No such sum row with id {id}')
        return 0
    else:
        sum_row = sum_row.iloc[0]
    
    if sum_row['quantity'] < 0:
        quantity = sum_row['quantity'] + 1
        update_rows('sum', set_clause=f"quantity = {quantity}", where_clause=f"id = {sum_row['id']}")
    
    if sum_row['interval_mode'] == 'relative':
        if sum_row['end_lag'] is not None:
            end_lag = sum_row['end_lag']
            end_date = datetime.datetime.now() - timeshift(end_lag)
        else:
            end_date = datetime.now()
    else:
        end_date = sum_row['end_date']
    
    start_date = end_date - sum_row['interval_length']
    
    match sum_row['sum_mode']:
        case -1:
            changes = read_rows(f'''SELECT SUM(new_quantity - old_quantity) AS total_changes FROM history
                WHERE change_time BETWEEN '{start_date}' AND '{end_date}' AND record_id = {sum_row['record_id']} AND new_quantity - old_quantity < 0 ''')
            return changes['total_changes'].iloc[0] if not changes.empty else 0

        case 0:
            changes = read_rows(f'''SELECT SUM(new_quantity - old_quantity) AS total_changes FROM history
                WHERE change_time BETWEEN '{start_date}' AND '{end_date}' AND record_id = {sum_row['record_id']} ''')
            return changes['total_changes'].iloc[0] if not changes.empty else 0

        case 1:
            changes = read_rows(f'''SELECT SUM(new_quantity - old_quantity) AS total_changes FROM history
                WHERE change_time BETWEEN '{start_date}' AND '{end_date}' AND record_id = {sum_row['record_id']} AND new_quantity - old_quantity > 0 ''')
            return changes['total_changes'].iloc[0] if not changes.empty else 0

    return 0


def execute_operation(operation):
    if operation == None: return False

    operation = re.findall(r'\d+|[a-zA-Z]+', operation)
    operation = [int(x) if x.isdigit() else x for x in operation]

    if len(operation) == 1 and isinstance(operation[0], int): return execute_sql_command(command=f'UPDATE record SET quantity = 0 WHERE ID = {operation[0]}')

    where_id_in = None

    if len(operation) > 2:
        where_id_in = ' WHERE id in ('
        for item in operation[2:]: where_id_in += f'{item},'
        where_id_in = where_id_in[:-1] + ')'

    table = 'record'
    for item in operation:
        match item:
            case 0: table = 'configuration'
            case 1: table = 'history'
            case 2: table = 'record'
            case 3: table = 'karma'
            case 4: table = 'frequency'
            case 5: table = 'command'
            case 6: table = 'sum'
            case 7: table = 'transfer'
            case 8: table = 'views'
            case 'e' | 'E': return sys.exit()
            case 'h' | 'H': return print_help()
            case 's' | 'S': return dump_db()
            case 'l' | 'L': return restore_db()
            case 'c' | 'C': return create_row(table)
            case 'r' | 'R': return read_rows(f'SELECT * FROM {table}', where_id_in, view_mode=True)
            case 'u' | 'U': return update_rows(table, set_clause=None, where_clause=None, where_id_in=where_id_in)
            case 'd' | 'D': return delete_rows(table, where_clause=None, where_id_in=where_id_in)
            case 'f' | 'F': return execute_sql_command_from_file()
            case 'a' | 'A': return activate_configuration(operation[1])
            case 'q' | 'Q': return execute_sql_command(command=input('Type the SQL command: '))
            #case 'o' | 'O': db=input('DB name at src/db/versions/ (without .sql): '); restore_db(db=db)

    return True
