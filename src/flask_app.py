from flask import Flask, request, redirect, url_for, render_template
import pandas as pd
from time import sleep
from datetime import datetime, timezone, timedelta
from threading import Thread
from backend.main import *

def startup():
    if check_exists_db() is not None:
        drop_db()
    create_db(); scheme_db(); restore_db(); insert_ifnot_db()

def karma_scheduler():
    while True:
        karma()
        dump_db()
        sleep(60)

def table_recognizer(command):
    command_parts = command.upper().split()
    try:
        return command_parts[command_parts.index('FROM') + 1]
    except Exception:
        return None 

app = Flask(__name__)

@app.route('/edit_table', methods=['POST'])
def edit_table():
    table_name = request.form['table_name']
    
    for key, new_value in request.form.items():
        if key not in ['table_name']:
            column, row_id = key.split('_')
            update_rows(table_name, set_clause=f"{column} = '{new_value}'", where_clause=f"id = {row_id}")
            dump_db()
    
    return redirect(url_for('show_lince'))

@app.get('/')
def show_lince():
    options = [
        [ 'App', 'Operations', 'Tables' ],
        [ '[E] Exit', '[C] Create', '[0] Configuration' ],
        [ '[H] Help', '[R] Read', '[1] History' ],
        [ '[S] Save DB', '[U] Update', '[2] Record' ],
        [ '[L] Load DB', '[D] Delete', '[3] Karma' ],
        [ '[A] Activate Config', '[Q] Query', '[4] Frequency' ],
        [ '', '[F] SQL File','[5] Command' ],
        [ '', '','[6] Sum' ],
        [ '', '','[7] Transfer' ],
        [ '', '','[8] View' ]
    ]
    options_df = pd.DataFrame(options).to_html(header=False, index=False, table_id='table')

    configuration_df = read_rows('SELECT * FROM configuration')
    configuration_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]

    tz = configuration_row['timezone']
    current_date = datetime.now(timezone(timedelta(hours=int(tz)))).strftime("%Y-%m-%d %H:%M:%S")

    view = read_rows(f'SELECT view, view_name FROM views WHERE id = {configuration_row["view_id"]}')
    view_name = view['view_name'].iloc[0]
    view = view['view'].iloc[0]
    records_df = read_rows(view, view_mode=True)

    table_name = table_recognizer(view)

    return render_template('index.html', options_table=options_df, table_data=records_df.to_dict(orient='records'), table_name=table_name, view_name=view_name, current_date=current_date)


@app.post('/')
def submit_operation():
    operation = request.form.get('operation_of_choice')
    if operation:
        execute_operation(operation)
        karma()
    return redirect(url_for('show_lince'))

if __name__ == '__main__':
    startup()
    
    karma_thread = Thread(target=karma_scheduler)
    karma_thread.daemon = True
    karma_thread.start()

    app.run(debug=True)
