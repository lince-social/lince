from backend.main import *
from tabulate import tabulate


def clear_screen():
    # return print(f'- Lince - Configuration {conf_id}')
    return os.system('clear')


def choose_operation():
    options = [
        [ 'App', 'Operations', 'Tables' ],
        [ '[E] Exit', '[C] Create', '[T0] Configuration' ],
        [ '[S] Save DB', '[R] Read', '[T1] Record' ],
        [ '[L] Load DB', '[U] Update', '[T2] Frequency' ],
        [ '[H] Help', '[D] Delete', '' ],
        [ '', '[Q] Query', '' ],
        [ '', '[F] SQL File','' ]
    ]

    print(tabulate(options, headers='firstrow', tablefmt='psql'))
    return input('Your choice: ')


def main():
    if check_exists_db() is not None:
        drop_db()
    create_db()
    scheme_db()
    restore_db()
    restore_db()

    configuration_df = read_rows('SELECT * FROM configuration')
    max_quantity_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]

    conf_id = max_quantity_row['id']
    conf_save_mode = max_quantity_row['save_mode']
    conf_view = max_quantity_row['view']
    conf_view_list = conf_view.split('|')

    column_information_mode = max_quantity_row['column_information_mode']
    # truncation = max_quantity_row['truncation']

    while True:
        bring_consequences()

        clear_screen()

        for command in conf_view_list:
            print(tabulate(read_rows(command.strip()), headers='keys', tablefmt='psql'))
            print()

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
