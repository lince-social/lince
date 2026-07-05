| Column Name | Data Type |
| ----------- | --------- |
| Id          | Number    |
| Quantity    | Number    |
| Query       | Text      |

The Query is an SQL command you can run and affect your current DNA.

*Example:* 

| Id | Quantity | Query                          |
| -- | -------- | ------------------------------ |
| 1  |          | Robert'); DROP TABLE users; -- |

It is referenced in Karma Condition and/or Consequence as the characters 'sql', followed by the id number, so this example would be 'sql1'.