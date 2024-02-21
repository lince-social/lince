import datetime
import psycopg2
from uuid import uuid4

conn = psycopg2.connect(
    host='localhost',
    port='5432',
    database='personallince',
    user='postgres',
    password='atencao')

cursor = conn.cursor()

def execute_query(query):
    cursor.execute(query)
    if query.strip().upper().startswith('SELECT'):
        return pd.DataFrame(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])
    else:
        return None

def display_table(table):
    st.subheader(f'Table: {table}')
    st.dataframe(execute_query(f'SELECT * FROM {table}'))


def insert_record(table, values):
    execute_query(f'INSERT INTO {table} VALUES {values}')
    conn.commit()


def update_record(table, set_clause, where_clause):
    execute_query(f'UPDATE {table} SET {set_clause} WHERE {where_clause}')
    conn.commit()


def delete_record(table_name, where_clause):
    execute_query(f'DELETE FROM {table} WHERE {where_clause}')
    conn.commit()


def execute_sql_file(file):
    queries = file.split(';')
    for query in queries:
        query = query.strip()
        if query:
            execute_query(query)
    conn.commit()


def check_and_update_cadastro():
    # get the current date and time
    now = datetime.datetime.now()

    # query the cadastro table and store the results in a pandas dataframe
    cadastro_df = execute_query('SELECT * FROM cadastro')

    # loop through each row of the cadastro table
    for index, row in cadastro_df.iterrows():
        # get the id, quantidade, and localizacao of the current cadastro
        id_cadastro = row['id']
        quantidade_cadastro = row['quantidade']
        localizacao_cadastro = row['localizacao']

        # query the sentinela table to find the matching id_cadastro_observado
        sentinela_df = execute_query(f'SELECT * FROM sentinela WHERE id_cadastro_observado = \'{id_cadastro}\'')
        
        # if there is a match, check the certa_quantidade_cadastro and alteracao_quantidade_cadastro
        if not sentinela_df.empty:
            certa_quantidade_cadastro = sentinela_df.iloc[0]['certa_quantidade_cadastro']
            alteracao_quantidade_cadastro = sentinela_df.iloc[0]['alteracao_quantidade_cadastro']

            # if the quantidade_cadastro is equal to the certa_quantidade_cadastro, update the quantidade_cadastro by adding or subtracting the alteracao_quantidade_cadastro
            if quantidade_cadastro == certa_quantidade_cadastro:
                new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
                update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')

        # query the periodicidade table to find the matching id_cadastro_alterado
        periodicidade_df = execute_query(f'SELECT * FROM periodicidade WHERE id_cadastro_alterado = \'{id_cadastro}\'')

        # if there is a match, check the periodos_desde_alteracao, periodicidade, tipo_periodicidade_dia_true_mes_false, data_inicio, and alteracao_quantidade_cadastro
        if not periodicidade_df.empty:
            periodos_desde_alteracao = periodicidade_df.iloc[0]['periodos_desde_alteracao']
            periodicidade = periodicidade_df.iloc[0]['periodicidade']
            tipo_periodicidade_dia_true_mes_false = periodicidade_df.iloc[0]['tipo_periodicidade_dia_true_mes_false']
            data_inicio = periodicidade_df.iloc[0]['data_inicio']
            alteracao_quantidade_cadastro = periodicidade_df.iloc[0]['alteracao_quantidade_cadastro']

            # calculate the difference between the current date and the data_inicio in days or months, depending on the tipo_periodicidade_dia_true_mes_false
            if tipo_periodicidade_dia_true_mes_false:
                # difference in days
                delta = (now - data_inicio).days
            else:
                # difference in months
                delta = (now.year - data_inicio.year) * 12 + (now.month - data_inicio.month)

            # if the difference is equal to or greater than the product of the periodos_desde_alteracao and the periodicidade, update the quantidade_cadastro by adding or subtracting the alteracao_quantidade_cadastro
            if delta >= periodos_desde_alteracao * periodicidade:
                new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
                update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')

                # also update the periodos_desde_alteracao by adding one
                new_periodos_desde_alteracao = periodos_desde_alteracao + 1
                update_record('periodicidade', f'periodos_desde_alteracao = {new_periodos_desde_alteracao}', f'id = \'{id_cadastro}\'')

# call the function to check and update the cadastro table
check_and_update_cadastro()

