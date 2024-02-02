import streamlit as st
import psycopg2
from uuid import uuid4
import pandas as pd 

conn = psycopg2.connect(
    host = 'localhost',
    port = '5432',
    database = 'personallince',
    user = 'postgres',
    password = 'atencao')

cursor = conn.cursor()

def execute_query(query):
    cursor.execute(query)
    return pd.DataFrame(cursor.fetchall(), columns=[desc[0] for desc in cursor.description])


def display_table(table_name):
    st.subheader(f'Table: {table_name}')
    st.dataframe(execute_query(f'SELECT * FROM {table_name}'))


def insert_record(table_name, values):
    rsor.execute_query(f'INSERT INTO {table_name} VALUES {values}')
    conn.commit()
    st.success(f'Record inserted into {table_name}')


def update_record(table_name, set_clause, where_clause):
    cursor.excecute_query(f'UPDATE {table_name} SET {set_clause} WHERE {where_clause}')
    conn.commit()
    st.success(f'Record updated in {table_name}')


def delete_record(table_name, where_clause):
    cursor.excecute_query(f'DELETE FROM {table_name} WHERE {where_clause}')
    conn.commit()
    st.success(f'Record deleted from {table_name}')


cursor = conn.cursor()
def execute_query(query):
    cursor.execute(query)
    result = cursor.fetchall()
    columns = [desc[0] for desc in cursor.description]
    df = pd.DataFrame(result, columns=columns)
    return df

def display_table(table_name):
    st.subheader(f"Table: {table_name}")
    query = f"SELECT * FROM {table_name}"
    df = execute_query(query)
    st.dataframe(df)

def insert_record(table_name, values):
    query = f"INSERT INTO {table_name} VALUES {values}"
    cursor.execute(query)
    conn.commit()
    st.success(f"Record inserted into {table_name}")

# Define a function to update an existing record in a table
def update_record(table_name, set_clause, where_clause):
    query = f"UPDATE {table_name} SET {set_clause} WHERE {where_clause}"
    cursor.execute(query)
    conn.commit()
    st.success(f"Record updated in {table_name}")

# Define a function to delete an existing record from a table
def delete_record(table_name, where_clause):
    query = f"DELETE FROM {table_name} WHERE {where_clause}"
    cursor.execute(query)
    conn.commit()
    st.success(f"Record deleted from {table_name}")
