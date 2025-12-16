== Example: Command

With the command table

Let's say you have a command:

| id  | quantity | name     | command        |
| --- | -------- | -------- | -------------- |
| 3   | 1        | Radarada | touch grass.el |

This command with ID 3 can be referenced in a Karma Consequence with 'c3'.

== Consequence

| id  | quantity | name                         | consequence |
| --- | -------- | ---------------------------- | ----------- |
| 2   |          | Radarada (Automatically set) | c3          |

We can reuse the Frequency, and Condition, creating just a Consequence and Karma to automate the running of this command.

== Frequency

| id  | quantity | days | next_date           |
| --- | -------- | ---- | ------------------- |
| 1   |          | 1    | 2030-01-01 10:00:00 |

== Condition

| id  | quantity | name  | condition |
| --- | -------- | ----- | --------- |
| 1   |          | Daily | -1 \* f1  |

== Karma

| id  | quantity | name                               | condition_id | operator | consequence_id |
| --- | -------- | ---------------------------------- | ------------ | -------- | -------------- |
| 1   |          | Daily Radarada (Automatically set) | 1            | =        | 2              |

This will daily run the Command 'touch grass.el'.

---

Alternativelly, you can make it so the Command is part of the Condition.

```bash
echo $(find ~/books/technology -type f | wc -l)
```

This command will output a number, which we can use inside a Condition.

== Command

| id  | quantity | name                   | command                                          |
| --- | -------- | ---------------------- | ------------------------------------------------ |
| 4   |          | Tech Books Count       | echo $(find ~/books/technology -type f \| wc -l) |
| 5   |          | Open Current Tech Book | pdfreader ~/books/techology/current/\*           |

== Condition

| id  | quantity | name           | condition      |
| --- | -------- | -------------- | -------------- |
| 3   |          | Read Tech Task | -1 \* f1 \* c4 |

== Consequence

| id  | quantity | name                   | consequence |
| --- | -------- | ---------------------- | ----------- |
| 3   |          | Open Current Tech Book | c5          |

== Karma

| id  | quantity | name                               | condition_id | operator | consequence_id |
| --- | -------- | ---------------------------------- | ------------ | -------- | -------------- |
| 1   |          | Daily Radarada (Automatically set) | 3            | =        | 3              |

That will automatically open the current technology book one is reading. If the automatic opening is not ideal one can set it so the Consequence is changing the quantity of a Record:

| id  | quantity | head           | body |
| --- | -------- | -------------- | ---- |
| 1   | -1       | Read Tech Book |      |

When typing 1 in operation the quantity of this Record will be zero. If you have a Condition that is 'rq1 == 0' that will be evaluated to '1' and can trigger the Consequence 'c5'. Though this method needs one Karma expression to run 'c5' if 'rq1 == 0' and another to make it so if 'rq1 == 0' Condition then 'rq1' Consequence, making 'rq1' be '1' and not triggering the infinite opening of the Tech Book.
