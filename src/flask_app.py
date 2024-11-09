from flask import Flask, request, redirect, url_for
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
        sleep(5)

app = Flask(__name__)

@app.get('/')
def show_lince():
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
    options_df = pd.DataFrame(options).to_html(header=True, table_id='table')

    # Fetch configuration details
    configuration_df = read_rows('SELECT * FROM configuration')
    configuration_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]
    tz = configuration_row['timezone']

    # Display view details
    view = read_rows(f'SELECT view, view_name FROM views WHERE id = {configuration_row["view_id"]}')
    view_name = view['view_name'].iloc[0]
    view = view['view'].iloc[0]
    records_df = read_rows(view, view_mode=True)
    records_df = records_df.to_html()

    # Render HTML with form
    return f"""
     <html>
        <head>
            <link rel="stylesheet" href="{url_for('static', filename='css/style.css')}">
        </head>
        <body>
            {options_df}
            <h3>{view_name}</h3>
            {records_df}
            <p>{datetime.now(timezone(timedelta(hours=int(tz)))).strftime("%Y-%m-%d %H:%M:%S")}</p>
            <form action="/" method="post">
                <label for="operation_of_choice">Enter an operation:</label>
                <input type="text" id="operation_of_choice" name="operation_of_choice" required>
                <button type="submit">Submit</button>
            </form>
        </body>
    </html>
    """

@app.post('/')
def submit_operation():
    operation = request.form.get('operation_of_choice')
    if operation:
        execute_operation(operation)  # Call the backend function with the user's input
    return redirect(url_for('show_lince'))

if __name__ == '__main__':
    startup()
    
    karma_thread = Thread(target=karma_scheduler)
    karma_thread.daemon = True
    karma_thread.start()

    app.run(debug=True)
