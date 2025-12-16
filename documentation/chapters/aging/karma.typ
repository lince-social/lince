== Karma

#table(
  [
    ["karma", "DATA TYPE"],
    ["id", "INTEGER"],
    ["quantity", "INTEGER"],
    ["name", "TEXT"],
    ["condition_id", "INTEGER"],
    ["operator", "TEXT"],
    ["consequence_id", "INTEGER"],
  ],
)

Karma is a condition checker and an action taker. A constructor of if/then, behavior, inside your DNA.
It does it by replacing symbols in the Condition that point to data with it's actual values, then evaluating it as a
mathematical equation and according to the Operator. If the operator is '=' it lets only non zero values take effect,
if it is '=\*' it doesnt have that constraint.

This is an introduction to Karma. The full description, plus examples, will take place a little further ahead.
This process is called a Delivery and it is run every 60 seconds, and sometimes instantly for some special cases.
== Karma Condition

#table(
  [
    ["COLUMN NAME", "DATA TYPE"],
    ["id", "INTEGER"],
    ["quantity", "INTEGER"],
    ["name", "TEXT"],
    ["condition", "TEXT"],
  ],
)

A Karma Condition is something checked by replacing parts of the string (text) with real values.
A record with id 1 has a quantity of 5. When we set a Karma Condition to be 'rq1' we are saying the value evaluated will be 5 (at that Delivery).
== Karma Consequence

#table(
  [
    ["COLUMN NAME", "DATA TYPE"],
    ["id", "INTEGER"],
    ["quantity", "INTEGER"],
    ["name", "TEXT"],
    ["consequence", "TEXT"],
  ],
)

The Karma Consequence works the same way as Condition but instead of getting values it is responsible for setting what
is supposed to change. It can be the activation of a Shell/SQL command, the changing of the value of a Record and more
in the future, like making transactions, changing Quantities of other tables like Configuration and Collection Views, or changing DNAs.

== Frequency

#table(
  [
    ["frequency", "DATA TYPE"],
    ["id", "INTEGER"],
    ["quantity", "INTEGER"],
    ["day_week", "INTEGER"],
    ["months", "INTEGER"],
    ["days", "INTEGER"],
    ["seconds", "INTEGER"],
    ["next_date", "STRING"],
    ["finish_date", "STRING"],
    ["catch_up_sum", "INTEGER"],
  ],
)

The frequency table holds account of a fixed period for returning a value of 1 or more. If a frequency occurs
every 1 day and 60 seconds it might start, for example at 2030-01-01 10:00:00. When that time comes, if this Frequency exists
in a Karma Condition it will be checked and updated, setting next_date to 2030-01-02 10:01:00.

The catch_up_sum is a multiplier, if many days have passed since the last check like 4, instead of each minute it returns one,
if catch up sum is 1 it will in the same Deliver jump the next_date until it has surpassed the current time returning the amount of times.
Anything else will not do that.

In Karma, Frequencies are represented by the letter 'f'. So setting in a Condition '-1 \* f1' means Karma Delivery
will check the returned number for the specific Frequency with Id 1 and multiply that by '-1'.

== Command

#table(
  [
    ["command", "DATA TYPE"],
    ["id", "INTEGER"],
    ["quantity", "INTEGER"],
    ["name", "TEXT"],
    ["command", "TEXT"],
  ],
)

The Command is a Shell command you can run in a bash Shell.

== Example:

#table(
  [
    ["id", "quantity", "command"],
    ["1", "", "touch grass.el"],
  ],
)

It is referenced in Karma Condition and/or Consequence as the letter 'c', followed by the id number, so this example would be 'c1'.


== Query

#table(
  columns: 2,
  [COLUMN NAME], [DATA TYPE],
  [id], [INTEGER],
  [quantity], [INTEGER],
  [query], [TEXT],
)

The Query is an SQL command you can run.

== Example:

#table(
  [
    ["id", "quantity", "query"],
    ["1", "", "Robert'); DROP TABLE users; --"],
  ],
)

It is referenced in Karma Condition and/or Consequence as the characters 'sql', followed by the id number, so this example would be 'sql1'.
== Sum

#table(
  [
    ["sum", "DATA TYPE"],
    ["id", "INTEGER"],
    ["quantity", "INTEGER"],
    ["record_id", "INTEGER"],
    ["interval_relative", "BOOL"],
    ["interval_length", "INTERVAL"],
    ["sum_mode", "INTEGER"],
    ["end_lag", "INTERVAL"],
    ["end_date", "TIMESTAMP"],
  ],
)

This table is responsible for returning a sum of change. Change of a record's quantity over time. You have the 'record_id' to know what record to look for.
'interval_length' is a period, can be 1 day, 8 months, etc. If 'interval_relative' is TRUE, will sum all the changes ending now,
starting at the past based on the 'interval_length' (ex: 'interval_length' of 8 months and 'interval_relative' is TRUE will make
it look at the past 8 months). If FALSE an 'end_date' will need to be supplied. so the past 8 months ending in a certain date.
'end_lag' will give a space between now and the end point of a relative interval. So if the 'interval_length' is 8 months and
'interval_relative' is TRUE, an 'end_lag' of 2 months will make the sum look at the 2 months back to 10 months back, starting 10 months ago,
ending 2 months ago. 'sum_mode' tells Lince what to sum, if 0 it's a delta, essentially doing a quantity now - quantity earlier.
If it's negative, it sums all negative changes, positive, idem.
