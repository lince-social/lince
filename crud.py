import streamlit as st
import psycopg2
from uuid import uuid4
import pandas as pd

conn = psycopg2.connect(
    host="localhost",
    port="5432",
    database="personallince",
    user="postgres",
    password="atencao")

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


def execute_sql_file(file_contents):
    queries = file_contents.split(';')
    for query in queries:
        query = query.strip()
        if query:
            execute_query(query)
    conn.commit()


tables= ["conta", "cadastro", "proposta_transferencia", "sentinela", "periodicidade"]

st.sidebar.title("CRUD App")
table = st.sidebar.selectbox("Select a table", tables)

display_table(table)

operation = st.sidebar.radio("Select an operation", ["Insert", "Update", "Delete",'Custom Query'])

st.sidebar.subheader(f"{operation} record")
if operation == "Insert":
    query = f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'"
    df = execute_query(query)
    values = {}
    for i, row in df.iterrows():
        col_name = row["column_name"]
        col_type = row["data_type"]
        if col_type == "uuid":
            value = uuid4()
        elif col_type == "boolean":
            value = st.sidebar.checkbox(col_name)
        else:
            value = st.sidebar.text_input(col_name)
        values[col_name] = value
    values = tuple(values.values())
    if st.sidebar.button("Insert"):
        insert_record(table, values)

elif operation == "Update":
    query = f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary"
    df = execute_query(query)
    pk = df.iloc[0, 0]
    pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be updated")
    query = f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'"
    df = execute_query(query)
    values = {}
    for i, row in df.iterrows():
        col_name = row["column_name"]
        col_type = row["data_type"]
        if col_name == pk:
            continue
        elif col_type == "boolean":
            value = st.sidebar.checkbox(col_name)
        else:
            value = st.sidebar.text_input(col_name)
        values[col_name] = value
    set_clause = ", ".join([f"{k} = '{v}'" for k, v in values.items() if v != ""])
    where_clause = f"{pk} = '{pk_value}'"
    if st.sidebar.button("Update"):
        update_record(table, set_clause, where_clause)

elif operation == "Delete":
    query = f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary"
    df = execute_query(query)
    pk = df.iloc[0, 0]
    pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be deleted")
    where_clause = f"{pk} = '{pk_value}'"
    if st.sidebar.button("Delete"):
        delete_record(table, where_clause)

elif operation == "Custom Query":
    sql_script = st.sidebar.text_area("Enter your SQL script:")
    if st.sidebar.button("Execute Query"):
        result = execute_query(sql_script)
        if result is not None:
            st.sidebar.text("Query executed successfully!")
            st.sidebar.dataframe(result)

elif operation == "Upload SQL File":
    uploaded_file = st.sidebar.file_uploader("Choose a .sql file", type="sql")
    if uploaded_file is not None:
        file_contents = uploaded_file.read().decode("utf-8")
        if st.sidebar.button("Execute SQL File"):
            execute_sql_file(file_contents)
            st.sidebar.text("SQL file executed successfully!")
else:
    st.sidebar.text("Select a valid operation.")
