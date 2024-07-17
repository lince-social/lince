import pandas as pd
from datetime import datetime, timedelta
#     df = a()
#     for each row on df:
#         next_period = df['next_period']

#         if next_period[:10] == datetime.today().date().strftime('%Y-%m-%d'):
#             df.next_period.datetime.advance.days(df['days'])

#     return df


def df_maker():
    data = {
        'id': [2, 3, 4],
        'next_period': ["2024-07-16 03:52:12.817932+00:0", "2024-07-17 03:52:12.817932+00:0", "2024-07-18 03:52:12.817932+00:0"],
        'days': [1, 2, 3],
        'record_id': [1, 2, 3],
        'quantity': [-1.0, -2.0, -3.0] }

    return pd.DataFrame(data)


def compare_date():
    df = df_maker()

    df['next_period'] = pd.to_datetime(df['next_period'])

    today = datetime.today().date()

    for index, row in df.iterrows():
        if row['next_period'].date() == today:
            new_date = row['next_period'] + timedelta(days=row['days'])
            df.at[index, 'next_period'] = new_date
    return df


print('--')
print('--')
print(df_maker())
print('--')
print(compare_date())
print('--')
print('--')



# import pandas as pd
# from datetime import datetime, timedelta


# def make()
# data = {
#     'id': [2, 3, 4],
#     'next_period': ["2024-07-16 03:52:12.817932+00:0", "2024-07-17 03:52:12.817932+00:0", "2024-07-18 03:52:12.817932+00:0"],
#     'days': [1, 2, 3],
#     'record_id': [1, 2, 3],
#     'quantity': [-1.0, -2.0, -3.0]
# }
# frequency_df = pd.DataFrame(data)

# frequency_df['next_period'] = pd.to_datetime(frequency_df['next_period'])

# today = datetime.today().date()

# conn = psycopg2.connect(
#     dbname="your_db_name",
#     user="your_db_user",
#     password="your_db_password",
#     host="your_db_host",
#     port="your_db_port"
# )

# sql_query = "SELECT * FROM records"
# record_df = pd.read_sql(sql_query, conn)


# for index, row in frequency_df.iterrows():
#     if row['next_period'].date() == today:
#         record_id = row['record_id']
#         new_quantity = row['quantity']
#         record_df.loc[record_df['id'] == record_id, 'quantity'] = new_quantity

# record_df.to_sql("records", conn, if_exists="replace", index=False)

# conn.close()

# print(record_df)

# x = 5
# y = -6

# x += y

# print(x)
