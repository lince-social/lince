# Import the required libraries
import streamlit as st
import psycopg2
from uuid import uuid4
import pandas as pd

#database = input("DATABASE: ")
#user = input("USER: ")
#password = input("PASSWORD: ")

# Connect to the PostgreSQL database
conn = psycopg2.connect(
    host="localhost",
    port="5432",
    database="personallince",
    user="postgres",
    password="atencao"
)

# Create a cursor object
cursor = conn.cursor()

# Define a function to execute SQL queries and return the results as a dataframe
def execute_query(query):
    cursor.execute(query)
    result = cursor.fetchall()
    columns = [desc[0] for desc in cursor.description]
    df = pd.DataFrame(result, columns=columns)
    return df

# Define a function to display a table and its data
def display_table(table_name):
    st.subheader(f"Table: {table_name}")
    query = f"SELECT * FROM {table_name}"
    df = execute_query(query)
    st.dataframe(df)

# Define a function to insert a new record into a table
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

# Define a list of table names
table_names = ["conta", "cadastro", "proposta_transferencia", "sentinela", "periodicidade"]

# Create a sidebar with a selectbox to choose a table
st.sidebar.title("CRUD App")
table = st.sidebar.selectbox("Select a table", table_names)

# Display the selected table and its data
display_table(table)

# Create a sidebar with a radio button to choose an operation
operation = st.sidebar.radio("Select an operation", ["Insert", "Update", "Delete"])

# Create a sidebar with input fields to enter the values for the operation
st.sidebar.subheader(f"{operation} record")
if operation == "Insert":
    # Get the column names and types for the selected table
    query = f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'"
    df = execute_query(query)
    # Create a dictionary to store the values for each column
    values = {}
    # Loop through the columns and create input fields
    for i, row in df.iterrows():
        col_name = row["column_name"]
        col_type = row["data_type"]
        # If the column is a UUID, generate a default value
        if col_type == "uuid":
            value = uuid4()
        # If the column is a boolean, create a checkbox
        elif col_type == "boolean":
            value = st.sidebar.checkbox(col_name)
        # Otherwise, create a text input
        else:
            value = st.sidebar.text_input(col_name)
        # Store the value in the dictionary
        values[col_name] = value
    # Format the values as a tuple
    values = tuple(values.values())
    # Create a button to execute the insert operation
    if st.sidebar.button("Insert"):
        insert_record(table, values)
elif operation == "Update":
    # Get the primary key column for the selected table
    query = f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary"
    df = execute_query(query)
    pk = df.iloc[0, 0]
    # Create an input field to enter the primary key value of the record to be updated
    pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be updated")
    # Get the column names and types for the selected table
    query = f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'"
    df = execute_query(query)
    # Create a dictionary to store the values for each column
    values = {}
    # Loop through the columns and create input fields
    for i, row in df.iterrows():
        col_name = row["column_name"]
        col_type = row["data_type"]
        # If the column is the primary key, skip it
        if col_name == pk:
            continue
        # If the column is a boolean, create a checkbox
        elif col_type == "boolean":
            value = st.sidebar.checkbox(col_name)
        # Otherwise, create a text input
        else:
            value = st.sidebar.text_input(col_name)
        # Store the value in the dictionary
        values[col_name] = value
    # Format the values as a set clause
    set_clause = ", ".join([f"{k} = '{v}'" for k, v in values.items() if v != ""])
    # Format the where clause
    where_clause = f"{pk} = '{pk_value}'"
    # Create a button to execute the update operation
    if st.sidebar.button("Update"):
        update_record(table, set_clause, where_clause)
elif operation == "Delete":
    # Get the primary key column for the selected table
    query = f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary"
    df = execute_query(query)
    pk = df.iloc[0, 0]
    # Create an input field to enter the primary key value of the record to be deleted
    pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be deleted")
    # Format the where clause
    where_clause = f"{pk} = '{pk_value}'"
    # Create a button to execute the delete operation
    if st.sidebar.button("Delete"):
        delete_record(table, where_clause)
