import psycopg2

def insert_data():
    try:
        connection = psycopg2.connect(user="postgres",
                                      password="1234",
                                      host="localhost",
                                      database="your_database_name")
        cursor = connection.cursor()
        postgres_insert_query = """ INSERT INTO cadastro (id_conta, titulo, descricao, localizacao, quantidade) VALUES (%s,%s,%s,%s,%s)"""
        id_conta = input("Enter ID Conta: ")
        titulo = input("Enter Titulo: ")
        descricao = input("Enter Descricao: ")
        localizacao = input("Enter Localizacao: ")
        quantidade = float(input("Enter Quantidade: "))
        record_to_insert = (id_conta, titulo, descricao, localizacao, quantidade)
        cursor.execute(postgres_insert_query, record_to_insert)
        connection.commit()
        print("Record inserted successfully into cadastro table")

    except (Exception, psycopg2.Error) as error:
        print("Failed to insert record into cadastro table", error)

    finally:
        # closing database connection.
        if connection:
            cursor.close()
            connection.close()
            print("PostgreSQL connection is closed")

def update_data():
    try:
        connection = psycopg2.connect(user="postgres",
                                      password="1234",
                                      host="localhost",
                                      database="your_database_name")
        cursor = connection.cursor()
        cadastro_id = input("Enter Cadastro ID to update: ")
        new_titulo = input("Enter new Titulo: ")
        new_descricao = input("Enter new Descricao: ")
        new_localizacao = input("Enter new Localizacao: ")
        new_quantidade = float(input("Enter new Quantidade: "))
        postgres_update_query = """UPDATE cadastro SET titulo = %s, descricao = %s, localizacao = %s, quantidade = %s WHERE id = %s"""
        record_to_update = (new_titulo, new_descricao, new_localizacao, new_quantidade, cadastro_id)
        cursor.execute(postgres_update_query, record_to_update)
        connection.commit()
        print("Record updated successfully in cadastro table")

    except (Exception, psycopg2.Error) as error:
        print("Failed to update record in cadastro table", error)

    finally:
        # closing database connection.
        if connection:
            cursor.close()
            connection.close()
            print("PostgreSQL connection is closed")

def delete_data():
    try:
        connection = psycopg2.connect(user="postgres",
                                      password="1234",
                                      host="localhost",
                                      database="your_database_name")
        cursor = connection.cursor()
        cadastro_id = input("Enter Cadastro ID to delete: ")
        postgres_delete_query = """DELETE FROM cadastro WHERE id = %s"""
        cursor.execute(postgres_delete_query, (cadastro_id,))
        connection.commit()
        print("Record deleted successfully from cadastro table")

    except (Exception, psycopg2.Error) as error:
        print("Failed to delete record from cadastro table", error)

    finally:
        # closing database connection.
        if connection:
            cursor.close()
            connection.close()
            print("PostgreSQL connection is closed")

def show_data():
    try:
        connection = psycopg2.connect(user="postgres",
                                      password="1234",
                                      host="localhost",
                                      database="your_database_name")
        cursor = connection.cursor()
        postgres_select_query = """SELECT * FROM cadastro"""
        cursor.execute(postgres_select_query)
        records = cursor.fetchall()

        for row in records:
            print("ID = ", row[0])
            print("ID Conta = ", row[1])
            print("Titulo = ", row[2])
            print("Descricao = ", row[3])
            print("Localizacao = ", row[4])
            print("Quantidade = ", row[5], "\n")

    except (Exception, psycopg2.Error) as error:
        print("Failed to fetch records from cadastro table", error)

    finally:
        # closing database connection.
        if connection:
            cursor.close()
            connection.close()
            print("PostgreSQL connection is closed")


while True:
    print("1. Insert Data")
    print("2. Update Data")
    print("3. Delete Data")
    print("4. Show Data")
    print("5. Exit")
    choice = int(input("Enter your choice: "))
    if choice == 1:
        insert_data()
    elif choice == 2:
        update_data()
    elif choice == 3:
        delete_data()
    elif choice == 4:
        show_data()
    elif choice == 5:
        break
    else:
        print("Invalid Choice")