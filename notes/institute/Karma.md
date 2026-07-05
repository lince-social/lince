Karma is a automation feature, a Condition checker and a Consequence bringer. A constructor of if/and/then, behavior, inside your Lince.

It does it by combining data from different possible sources and evaluating to some data, may be a number or text, then putting it through a filter, if it passes, something happens, may be your data changing, or a computer action being executed, maybe even a Transfer happening...

We have then three parts, a Condition that is evaluated, an Operator acting as a filter/judge/threshold and a Consequence.

Under the hood, to set a Condition to be something we write codes, but a good Lince implementation will give you a way to not have to know that. Pulling data, frequencies and commands by their names in a buttons and graphical way should be enough to create Karmas. If you see the technical side being explained, know that in an interface that will be abstracted out, it is there just as a deeper technical documentation.

| Karma         | Data Type |
| ------------- | --------- |
| Id            | Number    |
| Quantity      | Number    |
| Name          | Text      |
| Condition Id   | Number    |
| Operator      | Text      |
| Consequence Id | Number    |

The process of evaluating all those Karma can be called a Delivery, checking data, running commands, making math formulas, changing Records happens, by default, every 60 seconds. When you interact with a Record, all the Karma related to it is re-evaluated to check if the new state will bring new Consequences.

The general formula for a Karma is:
Condition -> Operator -> Consequence

A Condition is evaluated. Its value passes through an Operator, if it is valid: a Consequence happens.

*Example*

Condition: Quantity of Record A -> evaluated to be '10' (the current quantity)
Operator: Only numbers that are not zero.
Consequence: Quantity of Record B

In this example the quantity of Record A is 10, it is not zero, so it passes the Operator and brings that value to the Consequence, that is the quantity of Record B, making Record B have the quantity of Record A. Almost like a B = A.

*Technical*
The following explanation and the further Records that explain in detail the codes are for those that want to understand the technical part, in your day-to-day you won't have to deal with the codes, just the text: Names, Categories...

Under the hood it is a code like rq1 (Record A, with id 1) = rq2 (Record B, with id 2):

rq1 = rq2
Condition -> Consequence

In the programming part, when we evaluate things and bring consequences we search for the real parts of Lince based on the codes (rq1, rq2...).

The Operator '=' lets only non zero quantities pass, the Operator '=*' lets every number pass.