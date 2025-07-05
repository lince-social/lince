# Frequency

| frequency    | DATA STRUCTURE |
| ------------ | -------------- |
| id           | SERIAL         |
| quantity     | INTEGER        |
| day_week     | REAL           |
| months       | REAL           |
| days         | REAL           |
| seconds      | REAL           |
| next_date    | TIMESTAMP      |
| finish_date  | DATE           |
| catch_up_sum | INTEGER        |

'frequency' contains a plethora of ways for modeling a frequency, this is a table that makes more sense when one understands karma. If one wants to automate something that happens, changes every 4 days 9 hours and 53 seconds, finishing at 2029, it can be done so. 'day_week' is the day of the week, monday is 1. 'months', 'days' and 'seconds' are what they seem, 'next_date' is the column that changes when the time comes, advancing to a next date in accordance with the previous columns. 'finish_date' asserts that it will only run before a certain date, 'catch_up_sum' is a value that when negative and the 'next_date' is way behind the current date, the returned value will be of a certain amount, so instead of returning one, it will return the absolute value of this column. Example:

There is one frequency for every 60 seconds, starting now, but I have activated it with karma 10 minutes from now. With a 'catch_up_sum' of -2 it will return 2, it should return 10, for it's been 10 minutes stopped, and then return 1 every 1 minute, but with a 'catch_up_sum' of -2 it returns 2. If the 'catch_up_sum' is more than zero, it will multiply the value returned, so if instead of -2 it's '2.5, after 10 minutes from being stopped it will return 25, and 2.5 every subsequent minute.
