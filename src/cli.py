from backend.main import *
from tabulate import tabulate


def clear_and_print_header():
    os.system('clear')
    return print('- Lince -')


def print_operation_options():
    options = [
        [ 'App', 'Operations', 'Tables' ],
        [ '[E] Exit', '[C] Create', '[1] Record' ],
        [ '[S] Save DB', '[R] Read', '[2] Frequency' ],
        [ '[L] Load DB', '[U] Update', '' ],
        [ '[H] Help', '[D] Delete', '' ],
        [ '', '[Q] Query', '' ],
        [ '', '[F] SQL File','' ],
        ['','[Z] Zero Quantity', '']
    ]

    print()
    print('Menu')
    return print(tabulate(options, headers='firstrow', tablefmt='psql'))


def choose_operation():
    return input('Your choice: ')


def main():
    if check_exists_db() is not None:
        dump_db()
        drop_db()
    create_db()
    scheme_db()
    restore_db()
    restore_db()

    while True:
        execute_frequency_job()

        clear_and_print_header()
        print()
        print('Record')
        print(tabulate(read_rows('record'), headers='keys', tablefmt='psql'))

        print_operation_options()
        operation = choose_operation()

        if ('r' or 'R' or 'a' or 'A') in operation:
            print(tabulate(execute_operation(operation), headers='keys', tablefmt='psql')) 
            input('(Press anything to continue) ')
        else:
            execute_operation(operation)
        dump_db()

    return None

if __name__ == "__main__":
    main()
