| Column Name | Data Type |
| ----------- | --------- |
| Id          | Number    |
| Quantity    | Number    |
| Name        | Text      |
| Consequence | Text      |

The Karma Consequence works the same way as Condition but instead of getting values it is responsible for setting what is supposed to change. 

It can be the changing of a Record's properties like quantity (or head, body...), it can be a Record property, a Record sync or being the activation of terminal/SQL commands.

In the future it would be cool to make Transfers, changing Quantities of other tables like Configuration, Frequency, maybe even altering other Karma.