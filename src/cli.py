from backend.main import *
from tabulate import tabulate


def clear_screen():
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

    configuration_df = read_rows('SELECT * FROM configuration')
    max_quantity_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]

    save_mode = max_quantity_row['save_mode']
    view_list = [v.strip() for v in max_quantity_row['view'].split('|')]
    column_information_mode = max_quantity_row['column_information_mode']

    while True:
        clear_screen()

        bring_consequences()
        
        for command in view_list:
            print(tabulate(read_rows(command), headers='keys', tablefmt='psql'))
            print()

        operation = choose_operation()

        result = execute_operation(operation)

        if isinstance(result, pd.DataFrame):
            print(tabulate(result, headers='keys', tablefmt='psql', stralign='left')) 
            input('(Press anything to continue) ')

        if save_mode == 'Automatic':
            dump_db()


    return None

if __name__ == "__main__":
    main()
