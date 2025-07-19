# Karma Consequence

| COLUMN NAME | DATA TYPE |
| ----------- | --------- |
| id          | INTEGER   |
| quantity    | INTEGER   |
| name        | TEXT      |
| consequence | TEXT      |

The Karma Consequence works the same way as Condition but instead of getting values it is responsible for setting what is supposed to change. It can be the activation of a Shell/SQL command, the changing of the value of a Record and more in the future, like making transactions, changing Quantities of other tables like Configuration and Collection Views, or changing DNAs.
