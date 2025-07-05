# Views

| COLUMNS | DATA STRUCTURE |
| ------- | -------------- |
| id      | INT            |
| name    | STRING         |
| query   | STRING         |

Views are ways to select records.

A view will have a name, so it is human readable and a query to display different data.

The query is made with SQL allowing you to select what columns you want to see, filtered, ordered and much more, just the way you want it.

### Examples:

Let's grab the record table shown before.

| id  | quantity | head        | body            | location |
| --- | -------- | ----------- | --------------- | -------- |
| 1   | -1       | Eat Apple   | NULL            | NULL     |
| 2   | -1       | Apple       | Item, Food      | NULL     |
| 3   | 0        | Brush Teeth | Action, Hygiene | NULL     |
| 4   | 3        | Toothbrush  | Item, Hygiene   | NULL     |
| 5   | -1       | Meditate    | Action          | NULL     |

A view of things you need to buy may look like this:

| COLUMN | VALUE                                                                  |
| ------ | ---------------------------------------------------------------------- |
| id     | 1                                                                      |
| name   | Buy                                                                    |
| query  | SELECT \* FROM record WHERE LOWER(body) LIKE '%item%' AND quantity < 0 |

And in this case, will display only:

| id  | quantity | head  | body       | location |
| --- | -------- | ----- | ---------- | -------- |
| 2   | -1       | Apple | Item, Food | NULL     |

Since no other records with a body containing 'Item' (lower, upper, pascal case...) exist with a negative quantity.
