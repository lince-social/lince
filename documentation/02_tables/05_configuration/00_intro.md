# Configuration

## Table: `configuration`

| configuration | DATA STRUCTURE |
| ------------- | -------------- |
| id            | SERIAL         |
| quantity      | REAL           |
| language      | VARCHAR        |
| timezone      | VARCHAR        |

This table is responsible for changing the behavior of Lince. The 'quantity' sets the active configuration, with the value of 1, the other rows have it 0. 'save_mode' can be automatic or manual, happening only when the user demands it, or after every database change. 'view_id' sets the rows you will see, this is a reference to the id of a row in view. 'column_information_mode' can be 'silent for no information, 'short', for the data types and constraints or 'verbose' for the previous information plus a description of the functionality/usage of the column. Keymap is for changing the commands sent manually (TODO) so 'd' is not for deleting records but for 'd'escribing lince, printing it's documentation. 'truncation' is somewhat deprecated, used for truncating cells's contents. 'table_query' is too deprecated, somewhat, from the terminal days, but essentially it changes how a table is queried. 'language' and 'timezone' are for the language and timezone :).
