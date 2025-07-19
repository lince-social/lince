# History

> This is from the old Postgres Schema and idea of History, it is kept here whilst it is in TODO.

| history      | DATA TYPE |
| ------------ | --------- |
| id           | INTEGER   |
| record_id    | INTEGER   |
| change_time  | STRING    |
| old_quantity | FLOAT     |
| new_quantity | FLOAT     |

History is a table that automatically logs the change of a record's quantity, to use the 'sum' table.
