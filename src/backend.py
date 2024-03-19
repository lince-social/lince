from datetime import datetime
import psycopg2
from uuid import uuid4
import pandas as pd

conn = psycopg2.connect(
    host='localhost',
    port='5432',
    database='lince',
    user='postgres',
    password='atencao')

cursor = conn.cursor()

def execute_query(query):
    cursor.execute(query)
    if query.strip().upper().startswith('SELECT'):
        return pd.DataFrame(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])
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
    now = datetime.datetime.now()
    cadastro_df = execute_query('SELECT * FROM cadastro')

    for index, row in cadastro_df.iterrows():
        id_cadastro = row['id']
        quantidade_cadastro = row['quantidade']


        observacao_ponto_df = execute_query(f'SELECT * FROM observacao_ponto WHERE id_cadastro_observado = \'{id_cadastro}\'')
        if not observacao_ponto_df.empty:
            certa_quantidade_cadastro = observacao_ponto_df.iloc[0]['certa_quantidade_cadastro']
            alteracao_quantidade_cadastro = observacao_ponto_df.iloc[0]['alteracao_quantidade_cadastro']

            if quantidade_cadastro == certa_quantidade_cadastro:
                new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
                update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')


        observacao_anicca_df = execute_query(f'SELECT * FROM observacao_anicca WHERE id_cadastro_observado = \'{id_cadastro}\'')
        if not observacao_anicca_df.empty:
            certa_quantidade_cadastro = observacao_anicca_df.iloc[0]['certa_quantidade_cadastro']
            alteracao_quantidade_cadastro = observacao_anicca_df.iloc[0]['alteracao_quantidade_cadastro']

            if quantidade_cadastro == certa_quantidade_cadastro:
                new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
                update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')


        periodicidade_df = execute_query(f'SELECT * FROM periodicidade WHERE id_cadastro_alterado = \'{id_cadastro}\'')
        if not periodicidade_df.empty:
            periodos_desde_alteracao = periodicidade_df.iloc[0]['periodos_desde_alteracao']
            periodicidade = periodicidade_df.iloc[0]['periodicidade']
            tipo_periodicidade_dia_true_mes_false = periodicidade_df.iloc[0]['tipo_periodicidade_dia_true_mes_false']
            data_inicio = periodicidade_df.iloc[0]['data_inicio']
            alteracao_quantidade_cadastro = periodicidade_df.iloc[0]['alteracao_quantidade_cadastro']

            if tipo_periodicidade_dia_true_mes_false:
                delta = (now - data_inicio).days
            else:
                delta = (now.year - data_inicio.year) * 12 + (now.month - data_inicio.month)

            if delta >= periodos_desde_alteracao * periodicidade:
                new_quantidade_cadastro = quantidade_cadastro + alteracao_quantidade_cadastro
                update_record('cadastro', f'quantidade = {new_quantidade_cadastro}', f'id = \'{id_cadastro}\'')

                new_periodos_desde_alteracao = periodos_desde_alteracao + 1
                update_record('periodicidade', f'periodos_desde_alteracao = {new_periodos_desde_alteracao}', f'id = \'{id_cadastro}\'')

check_and_update_cadastro()

