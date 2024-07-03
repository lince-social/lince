import psycopg2

def create_connection_object(host = 'localhost', user = 'postgres', database = 'lince', password = '1', port = '5432'):
    return psycopg2.connect(
        host = host
        user = user,
        database = database,
        password = password,
        port = port)

# def other():
#     thingy = testing()

#     print(thingy.database)

# print("dicn")
# other()
