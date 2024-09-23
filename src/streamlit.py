import streamlit as st
from time import sleep

from backend.main import *


def main():
    if check_exists_db() is not None: drop_db()
    create_db(); scheme_db(); restore_db(); restore_db(); insert_ifnot_db()

    configuration_df = read_rows('SELECT * FROM configuration')
    configuration_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]
    save_mode = configuration_row['save_mode']
    column_information_mode = configuration_row['column_information_mode']
    view = read_rows(f'SELECT view FROM views WHERE id = {configuration_row['view_id']}')
    view = view['view'].iloc[0]
    view_list = view.split('|')

    karma()

    for command in view_list:
        command = command.strip()
        df = st.data_editor(read_rows(command))
        execute_operation(st.text_input("Your command:",""))

        # while True:
        #     sleep(1)

            


    return True


if __name__ == "__main__":
    main()    
