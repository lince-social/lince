== Sum

| sum               | DATA TYPE |
| ----------------- | -------------- |
| id                | INTEGER         |
| quantity          | INTEGER        |
| record_id         | INTEGER        |
| interval_relative | BOOL           |
| interval_length   | INTERVAL       |
| sum_mode          | INTEGER        |
| end_lag           | INTERVAL       |
| end_date          | TIMESTAMP      |

This table is responsible for returning a sum of change. Change of a record's quantity over time. You have the 'record_id' to know what record to look for.
'interval_length' is a period, can be 1 day, 8 months, etc. If 'interval_relative' is TRUE, will sum all the changes ending now, starting at the past based on the 'interval_length' (ex: 'interval_length' of 8 months and 'interval_relative' is TRUE will make it look at the past 8 months). If FALSE an 'end_date' will need to be supplied. so the past 8 months ending in a certain date. 'end_lag' will give a space between now and the end point of a relative interval. So if the 'interval_length' is 8 months and 'interval_relative' is TRUE, an 'end_lag' of 2 months will make the sum look at the 2 months back to 10 months back, starting 10 months ago, ending 2 months ago. 'sum_mode' tells Lince what to sum, if 0 it's a delta, essentially doing a quantity now - quantity earlier. If it's negative, it sums all negative changes, positive, idem.
