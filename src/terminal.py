from backend.main import *
from tabulate import tabulate


def clear_screen():
    return os.system('clear')
    return None


def print_operations():
    options = [
        [ 'App', 'Operations', 'Tables' ],
        [ '[E] Exit', '[C] Create', '[0] Configuration' ],
        [ '[H] Help', '[R] Read', '[1] History' ],
        [ '[S] Save DB', '[U] Update', '[2] Record' ],
        [ '[L] Load DB', '[D] Delete', '[3] Karma' ],
        [ '[AC] Activate Config', '[Q] Query', '[4] Frequency' ],
        [ '', '[F] SQL File','[5] Command' ],
        [ '', '','[6] Sum' ],
        [ '', '','[7] Transfer' ],
        [ '', '','[8] View' ]
    ]

    return print(tabulate(options, headers='firstrow', tablefmt='rounded_grid'))

def choose_operation():
    return input('Your choice: ')
    
def main():
    if check_exists_db() is not None:
        drop_db()
    create_db(); scheme_db(); restore_db(); insert_ifnot_db()

    while True:
        configuration_df = read_rows('SELECT * FROM configuration')
        configuration_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]
        save_mode = configuration_row['save_mode']
        column_information_mode = configuration_row['column_information_mode']
        view = read_rows(f'SELECT view FROM views WHERE id = {configuration_row["view_id"]}')
        view = view['view'].iloc[0]
        view_list = view.split('|')
        tz = configuration_row['timezone']

        clear_screen()

        karma()

        print_operations()
        
        for command in view_list:
            command = command.strip()
            print(tabulate(read_rows(command, view_mode=True), headers='keys', tablefmt='rounded_grid'))
            print()

        print(datetime.now(timezone(timedelta(hours=int(tz)))).strftime("%Y-%m-%d %H:%M:%S"), end=' | ')
        result = execute_operation(choose_operation())

        if isinstance(result, pd.DataFrame):
            print(tabulate(result, headers='keys', tablefmt='rounded_grid', stralign='left')) 
            input('(Press enter to continue) ')

        

        if save_mode == 'Automatic':
            dump_db()

    return None

if __name__ == "__main__":
    main()
