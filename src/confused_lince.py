





# import pandas as pd
# from uuid import uuid4
# from os.path import exists
# from datetime import datetime
# import psycopg2
# import streamlit as st
# import subprocess

# host = 'localhost'
# port = '5432'
# database_name = 'lince'
# user = 'postgres'
# password = 'atencao'

# def check_db_and_populate():
#     conn = None
#     try:
#         conn = psycopg2.connect(
#             host=host,
#             port=port,
#             user=user,
#             password=password)
#         cur = conn.cursor()
        
#         cur.execute(f"SELECT datname FROM pg_database WHERE datname = '{database_name}'")
#         result = cur.fetchone()
        
#         if result is None:
#             cur.execute(f"CREATE DATABASE {database_name}")
#             print(f"Database '{database_name}' created successfully.")
            
#             conn.close()
#             conn = psycopg2.connect(
#                 host=host,
#                 port=port,
#                 database=database_name,
#                 user=user,
#                 password=password
#             )
            
#             sql_file_path = 'postgre.sql'
#             if os.path.exists(sql_file_path):
#                 with open(sql_file_path, 'r') as file:
#                     sql_commands = file.read()
#                 cur.execute(sql_commands)
#                 conn.commit()
#                 print(f"Database '{database_name}' populated successfully.")
#             else:
#                 print(f"SQL file '{sql_file_path}' not found.")
#     except OperationalError as e:
#         print(f"The error '{e}' occurred")
#     finally:
#         if conn is not None:
#             conn.close()


# def generate_unique_filename(base_filename):
#     counter = 0
#     filename = base_filename
#     while os.path.exists(filename):
#         counter += 1
#         filename = f"{base_filename}{counter}.sql"
#     return filename


# def execute_query(query):
#     cursor.execute(query)
#     if query.strip().upper().startswith('SELECT'):
#         return pd.DataFrame(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])
#     return None

# def display_table(table):
#     st.subheader(f'Table: {table}')
#     st.dataframe(execute_query(f'SELECT * FROM {table}'))


# def insert_record(table, values):
#     execute_query(f'INSERT INTO {table} VALUES {values}')
#     conn.commit()


# def update_record(table, set_clause, where_clause):
#     execute_query(f'UPDATE {table} SET {set_clause} WHERE {where_clause}')
#     conn.commit()


# def delete_record(table, where_clause):
#     execute_query(f'DELETE FROM {table} WHERE {where_clause}')
#     conn.commit()


# def execute_sql_file(file):
#     queries = file.split(';')
#     for query in queries:
#         query = query.strip()
#         if query:
#             execute_query(query)
#     conn.commit()


# def check_and_update_cadastro():
#     now = datetime.datetime.now()
#     cadastro_df = execute_query('SELECT * FROM cadastro')

#     for index, row in cadastro_df.iterrows():
#         id_cadastro = row['id']
#         quantidade_cadastro = row['quantidade']


#         observacao_ponto_df = execute_query(f'SELECT * FROM observacao_ponto WHERE id_cadastro_observado = \'{id_cadastro}\'')
#         if not observacao_ponto_df.empty:
#             certa_quantidade_cadastro = observacao_ponto_df.iloc[0]['certa_quantidade_cadastro']
#             alteracao_quantidade_cadastro = observacao_ponto_df.iloc[0]['alteracao_quantidade_cadastro']

#             if quantidade_cadastro == certa_quantidade_cadastro:
#                 new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
#                 update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')


#         observacao_anicca_df = execute_query(f'SELECT * FROM observacao_anicca WHERE id_cadastro_observado = \'{id_cadastro}\'')
#         if not observacao_anicca_df.empty:
#             certa_quantidade_cadastro = observacao_anicca_df.iloc[0]['certa_quantidade_cadastro']
#             alteracao_quantidade_cadastro = observacao_anicca_df.iloc[0]['alteracao_quantidade_cadastro']

#             if quantidade_cadastro == certa_quantidade_cadastro:
#                 new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
#                 update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')


#         periodicidade_df = execute_query(f'SELECT * FROM periodicidade WHERE id_cadastro_alterado = \'{id_cadastro}\'')
#         if not periodicidade_df.empty:
#             periodos_desde_alteracao = periodicidade_df.iloc[0]['periodos_desde_alteracao']
#             periodicidade = periodicidade_df.iloc[0]['periodicidade']
#             tipo_periodicidade_dia_true_mes_false = periodicidade_df.iloc[0]['tipo_periodicidade_dia_true_mes_false']
#             data_inicio = periodicidade_df.iloc[0]['data_inicio']
#             alteracao_quantidade_cadastro = periodicidade_df.iloc[0]['alteracao_quantidade_cadastro']

#             if tipo_periodicidade_dia_true_mes_false:
#                 delta = (now - data_inicio).days
#             else:
#                 delta = (now.year - data_inicio.year) * 12 + (now.month - data_inicio.month)

#             if delta >= periodos_desde_alteracao * periodicidade:
#                 new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
#                 update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')

#                 new_periodos_desde_alteracao = periodos_desde_alteracao + 1
#                 update_record('periodicidade', f'periodos_desde_alteracao = {new_periodos_desde_alteracao}', f'id = \'{id_cadastro}\'')


# check_db_and_populate()
# conn = psycopg2.connect(
#     host=host,
#     port=port,
#     database=database_name,
#     user=user,
#     password=password)

# cursor = conn.cursor()

# table = st.sidebar.radio('Select a table', ['conta', 'cadastro', 'proposta_transferencia', 'sentinela', 'periodicidade'])
# operation = st.sidebar.radio('Select an operation', ['Insert', 'Update', 'Delete', 'Custom Query', 'SQL File', 'Database to .sql file'])

# display_table(table)

# if operation == 'Insert':
#     df = execute_query(f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'")
#     values = {}
    
#     for i, row in df.iterrows():
#         col_name = row['column_name']
#         col_type = row['data_type']
#         if col_type == 'uuid':
#             value = uuid4()
#         elif col_type == 'boolean':
#             value = st.sidebar.checkbox(col_name)
#         else:
#             value = st.sidebar.text_input(col_name)
#         values[col_name] = value
    
#     values = tuple(values.values())
    
#     if st.sidebar.button('Insert'):
#         insert_record(table, values)

# elif operation == 'Update':

#     df = execute_query(f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary")

#     pk = df.iloc[0, 0]
#     pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be updated")
    
#     df = execute_query(f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'")
    
#     for i, row in df.iterrows():
#         col_name = row["column_name"]
#         col_type = row["data_type"]
#         if col_name == pk:
#             continue
#         elif col_type == "boolean":
#             value = st.sidebar.checkbox(col_name)
#         else:
#             value = st.sidebar.text_input(col_name)
#         values[col_name] = value
    
#     set_clause = ", ".join([f"{k} = '{v}'" for k, v in values.items() if v != ""])
#     where_clause = f"{pk} = '{pk_value}'"
    
#     if st.sidebar.button("Update"):
#         update_record(table, set_clause, where_clause)

# elif operation == "Delete":
#     df = execute_query(f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary")

#     pk = df.iloc[0, 0]
#     pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be deleted")
    
#     where_clause = f"{pk} = '{pk_value}'"

#     if st.sidebar.button("Delete"):
#         delete_record(table, where_clause)

# elif operation == "Custom Query":
#     text = st.sidebar.text_area("Enter your SQL script:") 
    
#     if st.sidebar.button("Execute Query"):
#         result = execute_query(text)
#         if result is not None:
#             st.sidebar.dataframe(result)

# elif operation == "SQL File":
#     uploaded_file = st.sidebar.file_uploader("Choose a .sql file", type="sql")
    
#     if uploaded_file is not None:
#         file_contents = uploaded_file.read().decode("utf-8")
#         if st.sidebar.button("Execute SQL File"):
#             execute_sql_file(file_contents)
#             st.sidebar.text("SQL file executed successfully!")

# elif operation == 'Database to .sql file':
#     base_filename = 'lincedb'
#     unique_filename = generate_unique_filename(base_filename)
#     pg_dump_command = [ 'pg_dump', '-U', user, '-W', '-F', 'plain', '-f', unique_filename, database_name ]

#     if st.sidebar.button('Pg_dump database'):
#         result = subprocess.run(pg_dump_command, text=True, input=f'{password}\n')
#         if result.returncode == 0:
#             print(f"Database dump successful. File saved as {unique_filename}.")
#         else:
#             print(f"Database dump failed with error code {result.returncode}.")    

# else:
#     st.sidebar.text("Select a valid operation.")








# import os
# from pathlib import Path
# from datetime import date, datetime


# def clean_done_run_scripts_ajust_date():      
#     with open('~/.vida/tarefas.md', 'r') as recurrence_file:
#         recurrence_lines = recurrence_file.readlines()
#     with open('~/.vida/tarefas.md', 'w') as recurrence_file:

#         for recurrence_line in recurrence_lines:
#             recurrence_line = recurrence_line.strip()

#             if not recurrence_line.startswith('- [x]'): recurrence_file.write(recurrence_line+"\n")

#             if recurrence_line.startswith('- [ ]') and 'every' in recurrence_line:
#                 recurrence_due_date = datetime.strptime(f"{recurrence_line[-10:]}", "%Y-%m-%d").date()
#                 # recurrence_
#                 for root, dirs, files in os.walk('/home/eduardo/.vida/automation/'):
#                     for file in files:
#                         if file != os.path.basename(__file__) and Path(file).stem in recurrence_line and (recurrence_due_date < date.today()):
#                             os.system(f'python {file}')


# '''
# # AGORA


# # SEI LA
# weeks:
# when done:

# every 2 weeks: weeks 2

# every 2 weeks on fridays: weeks 2 - day

# if today monday, till friday is 4 days, next friday is 4+7 = 11

# days = times * week - (days till next day)


# # COMPLETO
# if has 'every' and date:
#     if line starts with - [x]:

#             quantity = number after 'every'
#             if has days:
#                 dateshift = datetime.shift(days=quantity)
#             if has months
#                 dateshift = datetime.shift(months=quantity)
#             if has years
#                 dateshift = datetime.shift(years=quantity)

#             if has weeks
#                 for any day of the week bewteen 'on' and date
#                     day_of_week = date.today().of_week()
#                     weekday_numbers = {monday: 0, thursday:1, wednsday:2, tuesday:3, friday:4, saturday:5, sunday:6}
#                     quantity = 1 + weekday_numbers[date:lowest_value] weekday_numbers[day_of_week:value]
#                 dateshift = datetime.shift(days=quantity)



#             date = date + dateshift

#             replace x with space
#             write line

    
#     if line starts with - [ ]:
#             if date is today or less:
#                 if script == name:
#                     run script
#                     if success:
#                         replace space with x
# else:
#     if starts with - [x]
#         delete line
#     else:
#         write line

# '''

# if __name__ == "__main__":
#     clean_done_run_scripts_ajust_date()
