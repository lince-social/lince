def create_row(table=5):
    a = f"hello {table}"
    other(a)
    return True


def other(command):
    return print(command)

b = 's'
create_row(table = f'{b}')
