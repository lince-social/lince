| Column Name | Data Type |
| ----------- | --------- |
| Id          | Number    |
| Quantity    | Number    |
| Name        | Text      |
| Condition   | Text      |

A Karma Condition is something checked by replacing parts of the string (text) with real values.
A record with id 1 has a quantity of 5. When we set a Karma Condition to be 'rq1' we are saying the value evaluated will be 5 (at that Karma Delivery).

There are a lot of parts that can be a Condition: a Record's properties, a Frequency, the result of a terminal/SQL Command, or a Sum of the changing of a Record's quantity over a custom/dynamic time period