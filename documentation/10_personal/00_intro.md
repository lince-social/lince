# Author's Style

# Personal Note & Tips:

Modeling all items and actions to perform takes a while. Making a Lince DNA, understanding the possibilities is to me a long term effort, ever increasing the value you get from computers. The possibilites are vast, limited by one's domain of computers and imagination, when combined, wizardry skills arise.

From a usage standpoint, I recommend having several views, one for each table and a personal view for tasks, mine is:

```sql
SELECT * FROM record WHERE quantity < 0 AND (LOWER(body) LIKE '%task%' OR LOWER(body) LIKE '%item%') ORDER BY quantity ASC, body ASC, head ASC
```

I also use the 'body' column in 'record' table as a tag holder. So Record's bodies with Items and Tasks that have negative quantities appear in my 'Negatives' view, it is the one I most use.
