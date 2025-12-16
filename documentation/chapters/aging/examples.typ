== Examples

So you now know the parts of Karma. It's time to put everything together. Lets take this example:

== Record

#table(
  [
    ["id", "quantity", "head", "body"],
    ["1", "0", "Exercise", ""],
  ],
)

== Frequency

#table(
  [
    ["id", "quantity", "days", "next_date"],
    ["1", "", "1", "2030-01-01 10:00:00"],
  ],
)

== Condition

#table(
  [
    ["id", "quantity", "name", "condition"],
    ["1", "", "Daily", "-1 \* f1"],
  ],
)

== Consequence

#table(
  [
    ["id", "quantity", "name", "consequence"],
    ["1", "", "Exercise (Automatically set)", "rq1"],
  ],
)

== Karma

#table(
  [
    ["id", "quantity", "name", "condition_id", "operator", "consequence_id"],
    ["1", "", "Daily Exercise (Automatically set)", "1", "=", "1"],
  ],
)

With this data, every Karma Delivery (60 seconds) will check the validity of each condition. Frequency with id 1 'f1' will
return 0 in all minutes of the day safe for one time, where it will return 1. Since the operator of the Karma with id 1 is '='
the expression in it's Condition '-1 \* f1' will almost always be '-1 \* 0' and will not have a consequence.

In the Karma delivery that is past the next_date of Frequency, the next_date is updated and the Condition is '-1 \* 1'. Since that
is equal to '-1' the Consequence 'rq1' will change the quantity of Record with Id 1, turning it into '-1'.

We go from this:

| id  | quantity | head     | body |
| --- | -------- | -------- | ---- |
| 1   | 0        | Exercise |      |

To this:

#table(
  [
    ["id", "quantity", "head", "body"],
    ["1", "-1", "Exercise", ""],
  ],
)

One could make Conditions like '(rq1 - 1) \* f1' and a Consequence of 'rq2' so the number would not be fixated at -1, but would decrease
by one every day based on rq1. The expressions can become very complicated, but unless you are doing finance, joinning multiple sources
of data into two or three records like monthly costs/gains and runaway you will do simple expressions.

== Example: Command

With the command table

Let's say you have a command:

#table(
  [
    ["id", "quantity", "name", "command"],
    ["3", "1", "Radarada", "touch grass.el"],
  ],
)

This command with ID 3 can be referenced in a Karma Consequence with 'c3'.

== Consequence

#table(
  [
    ["id", "quantity", "name", "consequence"],
    ["2", "", "Radarada (Automatically set)", "c3"],
  ],
)

We can reuse the Frequency, and Condition, creating just a Consequence and Karma to automate the running of this command.

== Frequency

#table(
  [
    ["id", "quantity", "days", "next_date"],
    ["1", "", "1", "2030-01-01 10:00:00"],
  ],
)

== Condition

#table(
  [
    ["id", "quantity", "name", "condition"],
    ["1", "", "Daily", "-1 \* f1"],
  ],
)

== Karma

#table(
  [
    ["id", "quantity", "name", "condition_id", "operator", "consequence_id"],
    ["1", "", "Daily Radarada (Automatically set)", "1", "=", "2"],
  ],
)

This will daily run the Command 'touch grass.el'.

---

Alternativelly, you can make it so the Command is part of the Condition.

```bash
echo $(find ~/books/technology -type f | wc -l)
```

This command will output a number, which we can use inside a Condition.

== Command

#table(
  [
    ["id", "quantity", "name", "command"],
    ["4", "", "Tech Books Count", "echo \$(find ~/books/technology -type f \\| wc -l)"],
    ["5", "", "Open Current Tech Book", "pdfreader ~/books/techology/current/\*"],
  ],
)

== Condition

#table(
  [
    ["id", "quantity", "name", "condition"],
    ["3", "", "Read Tech Task", "-1 \* f1 \* c4"],
  ],
)

== Consequence

#table(
  [
    ["id", "quantity", "name", "consequence"],
    ["3", "", "Open Current Tech Book", "c5"],
  ],
)

== Karma

#table(
  [
    ["id", "quantity", "name", "condition_id", "operator", "consequence_id"],
    ["1", "", "Daily Radarada (Automatically set)", "3", "=", "3"],
  ],
)

That will automatically open the current technology book one is reading. If the automatic opening is not ideal one
can set it so the Consequence is changing the quantity of a Record:

#table(
  [
    ["id", "quantity", "head", "body"],
    ["1", "-1", "Read Tech Book", ""],
  ],
)

When typing 1 in operation the quantity of this Record will be zero. If you have a Condition that is 'rq1 == 0'
that will be evaluated to '1' and can trigger the Consequence 'c5'. Though this method needs one Karma expression
to run 'c5' if 'rq1 == 0' and another to make it so if 'rq1 == 0' Condition then 'rq1' Consequence,
making 'rq1' be '1' and not triggering the infinite opening of the Tech Book.
