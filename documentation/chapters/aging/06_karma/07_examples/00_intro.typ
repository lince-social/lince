== Examples

So you now know the parts of Karma. It's time to put everything together. Lets take this example:

== Record

| id  | quantity | head     | body |
| --- | -------- | -------- | ---- |
| 1   | 0        | Exercise |      |

== Frequency

| id  | quantity | days | next_date           |
| --- | -------- | ---- | ------------------- |
| 1   |          | 1    | 2030-01-01 10:00:00 |

== Condition

| id  | quantity | name  | condition |
| --- | -------- | ----- | --------- |
| 1   |          | Daily | -1 \* f1  |

== Consequence

| id  | quantity | name                         | consequence |
| --- | -------- | ---------------------------- | ----------- |
| 1   |          | Exercise (Automatically set) | rq1         |

== Karma

| id  | quantity | name                               | condition_id | operator | consequence_id |
| --- | -------- | ---------------------------------- | ------------ | -------- | -------------- |
| 1   |          | Daily Exercise (Automatically set) | 1            | =        | 1              |

With this data, every Karma Delivery (60 seconds) will check the validity of each condition. Frequency with id 1 'f1' will return 0 in all minutes of the day safe for one time, where it will return 1. Since the operator of the Karma with id 1 is '=' the expression in it's Condition '-1 \* f1' will almost always be '-1 \* 0' and will not have a consequence.

In the Karma delivery that is past the next_date of Frequency, the next_date is updated and the Condition is '-1 \* 1'. Since that is equal to '-1' the Consequence 'rq1' will change the quantity of Record with Id 1, turning it into '-1'.

We go from this:

| id  | quantity | head     | body |
| --- | -------- | -------- | ---- |
| 1   | 0        | Exercise |      |

To this:

| id  | quantity | head     | body |
| --- | -------- | -------- | ---- |
| 1   | -1       | Exercise |      |

One could make Conditions like '(rq1 - 1) \* f1' and a Consequence of 'rq2' so the number would not be fixated at -1, but would decrease by one every day based on rq1. The expressions can become very complicated, but unless you are doing finance, joinning multiple sources of data into two or three records like monthly costs/gains and runaway you will do simple expressions.
