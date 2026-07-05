| Frequency  | Data Type |
| ---------- | --------- |
| Id         | Number    |
| Quantity   | Number    |
| DayWeek    | Number    |
| Months     | Number    |
| Days       | Number    |
| Seconds    | Number    |
| NextDate   | Text      |
| FinishDate | Text      |
| CatchUpSum | Number    |

The frequency table holds account of a fixed period for returning a value of 1 or more. If a frequency occurs every 1 day and 60 seconds it might start, for example at 2030-01-01 10:00:00. When that time comes, if this Frequency exists in a Karma Condition it will be checked and updated, setting next_date to 2030-01-02 10:01:00.

The catch_up_sum is a multiplier, if many days have passed since the last check like 4, instead of returning one each minute, it catches up to the present in 'next_date' and returns the periods passed or only one.

In Karma, Frequencies are represented by the letter 'f'. So setting in a Condition '-1 * f1' means Karma Delivery will check the returned number for the specific Frequency with Id 1 and multiply that by '-1'.