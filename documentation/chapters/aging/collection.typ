== Collection

#table(
  columns: 2,
  [COLUMNS], [DATA TYPE],
  [id], [INTEGER],
  [name], [TEXT],
  [quantity], [INTEGER],
)

A Collection is the name of a list of Views. Only one Collection is active at a time, with quantity 1, the rest is 0.

== Views

#table(
  columns: 2,
  [COLUMNS], [DATA TYPE],
  [id], [INTEGER],
  [name], [TEXT],
  [query], [TEXT],
)

Views are ways to select data, to see what records exist with filters or ordering... They can also be used to see what Collections, Karma,
Configurations... exist.

A view will have a name, so it is human readable and a query to display different data.

The query is made with SQL allowing you to select what columns you want to see, filtered, ordered and much more, just the way you want it.

The 'query' of a view can also be a special code, like 'karma_view', in the HTML (Web Browser) frontend. It is a specially constructed html
to interact with Karma in a more complete and sane way, to bring ergonomics to day-to-day Lince. Instead of returning an automatically
constructed table with data from an SQL query, the Backend adds customized HTML components to the page sent to the user (normal frontend behavior).

*Examples:* \

Let's grab the record table shown before.

#table(
  columns: 5,
  [id], [quantity], [head], [body], [location],
  [1], [-1], [Eat Apple], [NULL], [NULL],
  [2], [-1], [Apple], [Item, Food], [NULL],
  [3], [0], [Brush Teeth], [Action, Hygiene], [NULL],
  [4], [3], [Toothbrush], [Item, Hygiene], [NULL],
  [5], [-1], [Meditate], [Action], [NULL],
)

A view of things you need to buy may look like this:

#table(
  columns: 2,
  [COLUMN], [VALUE],
  [id], [1],
  [name], [Buy],
  [query],
  [SELECT \* FROM record WHERE LOWER(body) LIKE '%item%' AND quantity < 0],
)

And in this case, will display only:

#table(
  columns: 5,
  [id], [quantity], [head], [body], [location],
  [2], [-1], [Apple], [Item, Food], [NULL],
)

Since no other records with a body containing 'Item' (lower, upper, pascal case...) exist with a negative quantity.

*Collection View*

#table(
  columns: 2,
  [COLUMNS], [DATA TYPE],
  [id], [INTEGER],
  [quantity], [INTEGER],
  [collection_id], [INTEGER],
  [view_id], [INTEGER],
)

A Collection View is an intermediate table for grouping Views into a Collection.
The quantity is the order in which they appear and if positive they are shown, negative ones are not.
