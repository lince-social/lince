from flask import Flask, render_template, jsonify
from backend.main import karma, read_rows
import time
import threading

app = Flask(__name__)

view_data = []

def update_view():
    global view_data
    while True:
        karma()
        
        configuration_df = read_rows('SELECT * FROM configuration')
        configuration_row = configuration_df[configuration_df['quantity'] == configuration_df['quantity'].max()].iloc[0]
        view = read_rows(f'SELECT view FROM views WHERE id = {configuration_row["view_id"]}')
        view = view['view'].iloc[0]
        view_list = view.split('|')

        view_data = [read_rows(command.strip()).to_dict(orient='records') for command in view_list]
        
        time.sleep(1)

@app.route('/')
def index():
    return render_template('index.html', data=view_data)

@app.route('/data')
def data():
    return jsonify(view_data)

if __name__ == '__main__':
    threading.Thread(target=update_view, daemon=True).start()
    
    app.run(debug=True)

