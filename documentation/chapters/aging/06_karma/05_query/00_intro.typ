== Query

| COLUMN NAME | DATA TYPE |
| ----------- | --------- |
| id          | INTEGER   |
| quantity    | INTEGER   |
| query       | TEXT      |

The Query is an SQL command you can run.

== Example:

| id  | quantity | query |
| --- | -------- | -------------- |
| 1   |          | Robert'); DROP TABLE users; -- |

It is referenced in Karma Condition and/or Consequence as the characters 'sql', followed by the id number, so this example would be 'sql1'.
