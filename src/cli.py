from backend.main import *
from tabulate import tabulate


def clear_and_print_header(conf_name, conf_save_mode):
    os.system('clear')
    return print(f'- Lince - Configuration: {conf_name} | Save Mode: {conf_save_mode}')


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

    configuration_df = read_rows('configuration')
    max_quantity_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]

    conf_name = max_quantity_row['name']
    conf_save_mode = max_quantity_row['save_mode']
    # view = max_quantity_row['view']
    # column_information = max_quantity_row['column_information']
    # keymap = max_quantity_row['keymap']
    # truncation = max_quantity_row['truncation']
    
    while True:
        execute_frequency_job()

        clear_and_print_header(conf_name, conf_save_mode)
        print()
        print('Record')
        print(tabulate(read_rows('record'), headers='keys', tablefmt='psql'))

        print_operation_options()
        operation = choose_operation()

        result = execute_operation(operation)

        if isinstance(result, pd.DataFrame):
            print(tabulate(result, headers='keys', tablefmt='psql', stralign='left')) 
            input('(Press anything to continue) ')

        if conf_save_mode == 'Automatic':
            dump_db()

    return None

if __name__ == "__main__":
    main()
