# Karma

| karma          | DATA TYPE |
| -------------- | --------- |
| id             | INTEGER   |
| quantity       | INTEGER   |
| name           | TEXT      |
| condition_id   | INTEGER   |
| operator       | TEXT      |
| consequence_id | INTEGER   |

Karma is a condition checker and an action taker. A constructor of if/then, behavior, inside your DNA. It does it by replacing symbols in the Condition that point to data with it's actual values, then evaluating it as a mathematical equation and according to the Operator. If the operator is '=' it lets only non zero values take effect, if it is '=*' it doesnt have that constraint.

This is an introduction to Karma. The full description, plus examples, will take place a little further ahead. This process is called a Delivery and it is run every 60 seconds, and sometimes instantly for some special cases.
