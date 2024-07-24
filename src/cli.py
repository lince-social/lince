from backend.main import *
from tabulate import tabulate


def clear_and_print_header():
    os.system('clear')
    return print('- Lince -')


def print_operation_options():
    options = [
        [ 'App', 'Operations', 'Tables' ],
        [ '[E] Exit', '[C] Create', '[T0] Configuration' ],
        [ '[S] Save DB', '[R] Read', '[T1] Record' ],
        [ '[L] Load DB', '[U] Update', '[T2] Frequency' ],
        [ '[H] Help', '[D] Delete', '' ],
        [ '', '[Q] Query', '' ],
        [ '', '[F] SQL File','' ]
    ]

    print()
    print('Menu')
    return print(tabulate(options, headers='firstrow', tablefmt='psql'))


def choose_operation():
    return input('Your choice: ')


def main():
    if check_exists_db() is not None:
        drop_db()
    create_db()
    scheme_db()
    restore_db()
    restore_db()

    # configuration_df = blabla
    # save_mode = configuration_df['save_mode']

    while True:
        execute_frequency_job()

        clear_and_print_header()
        print()
        print('Record')
        print(tabulate(read_rows('record'), headers='keys', tablefmt='psql'))

        print_operation_options()
        operation = choose_operation()

        result = execute_operation(operation)

        if isinstance(result, pd.DataFrame):
            print(tabulate(result, headers='keys', tablefmt='psql')) 
            input('(Press anything to continue) ')

        # if save_mode == 'automatic':
        dump_db()

    return None

if __name__ == "__main__":
    main()
