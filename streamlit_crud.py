import streamlit as st
import pandas as pd
import condition_changer as cc

table = st.sidebar.radio('Select a table', ['conta', 'cadastro', 'proposta_transferencia', 'sentinela', 'periodicidade'])
operation = st.sidebar.radio('Select an operation', ['Insert', 'Update', 'Delete', 'Custom Query', 'SQL File'])
display_table(table)

if operation == 'Insert':
    df = execute_query(f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'")
    values = {}
    
    for i, row in df.iterrows():
        col_name = row['column_name']
        col_type = row['data_type']
        if col_type == 'uuid':
            value = uuid4()
        elif col_type == 'boolean':
            value = st.sidebar.checkbox(col_name)
        else:
            value = st.sidebar.text_input(col_name)
        values[col_name] = value
    
    values = tuple(values.values())
    
    if st.sidebar.button('Insert'):
        insert_record(table, values)

elif operation == 'Update':
    values = {}
    df = execute_query(f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary")

    pk = df.iloc[0, 0]
    pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be updated")
    
    df = execute_query(f"SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'")
    
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
    df = execute_query(f"SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) WHERE i.indrelid = '{table}'::regclass AND i.indisprimary")

    pk = df.iloc[0, 0]
    pk_value = st.sidebar.text_input(f"Enter the {pk} of the record to be deleted")
    
    where_clause = f"{pk} = '{pk_value}'"

    if st.sidebar.button("Delete"):
        delete_record(table, where_clause)

elif operation == "Custom Query":
    text = st.sidebar.text_area("Enter your SQL script:") 
    
    if st.sidebar.button("Execute Query"):
        result = execute_query(text)
        if result is not None:
            st.sidebar.dataframe(result)

elif operation == "SQL File":
    uploaded_file = st.sidebar.file_uploader("Choose a .sql file", type="sql")
    
    if uploaded_file is not None:
        file_contents = uploaded_file.read().decode("utf-8")
        if st.sidebar.button("Execute SQL File"):
            execute_sql_file(file_contents)
            st.sidebar.text("SQL file executed successfully!")
else:
    st.sidebar.text("Select a valid operation.")
