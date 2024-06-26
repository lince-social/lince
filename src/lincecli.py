import os
from pathlib import Path
from datetime import date, datetime


def clean_done_run_scripts_ajust_date():      
    with open('~/.vida/tarefas.md', 'r') as recurrence_file:
        recurrence_lines = recurrence_file.readlines()
    with open('~/.vida/tarefas.md', 'w') as recurrence_file:

        for recurrence_line in recurrence_lines:
            recurrence_line = recurrence_line.strip()

            if not recurrence_line.startswith('- [x]'): recurrence_file.write(recurrence_line+"\n")

            if recurrence_line.startswith('- [ ]') and 'every' in recurrence_line:
                recurrence_due_date = datetime.strptime(f"{recurrence_line[-10:]}", "%Y-%m-%d").date()
                # recurrence_
                for root, dirs, files in os.walk('/home/eduardo/.vida/automation/'):
                    for file in files:
                        if file != os.path.basename(__file__) and Path(file).stem in recurrence_line and (recurrence_due_date < date.today()):
                            os.system(f'python {file}')


'''
# AGORA


# SEI LA
weeks:
when done:

every 2 weeks: weeks 2

every 2 weeks on fridays: weeks 2 - day

if today monday, till friday is 4 days, next friday is 4+7 = 11

days = times * week - (days till next day)


# COMPLETO
if has 'every' and date:
    if line starts with - [x]:

            quantity = number after 'every'
            if has days:
                dateshift = datetime.shift(days=quantity)
            if has months
                dateshift = datetime.shift(months=quantity)
            if has years
                dateshift = datetime.shift(years=quantity)

            if has weeks
                for any day of the week bewteen 'on' and date
                    day_of_week = date.today().of_week()
                    weekday_numbers = {monday: 0, thursday:1, wednsday:2, tuesday:3, friday:4, saturday:5, sunday:6}
                    quantity = 1 + weekday_numbers[date:lowest_value] weekday_numbers[day_of_week:value]
                dateshift = datetime.shift(days=quantity)



            date = date + dateshift

            replace x with space
            write line

    
    if line starts with - [ ]:
            if date is today or less:
                if script == name:
                    run script
                    if success:
                        replace space with x
else:
    if starts with - [x]
        delete line
    else:
        write line

'''

if __name__ == "__main__":
    clean_done_run_scripts_ajust_date()
