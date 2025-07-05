# Author's Style

# Personal Note & Tips:

Modeling all items and actions to perform takes a while. Making a Lince instance, understanding the possibilities is to me a long term effort, ever increasing the value you get from computers. The possibilites are vast, limited by one's domain of computers and imagination, when combined, wizardry skills arise.

From a usage standpoint, I recommend having several views, one for each table and a personal view for tasks, mine is:

```sql
SELECT id, head FROM record WHERE quantity < 0 or LOWER(body) IS NULL
```

I also use the 'body' column in 'record' table as a tag holder, so every area in my life has either 'Action' or 'Item' and 'AREA'. With that, I have a view for every area and change configs rapidly with a2, a4, a3 so I can view the commands for running things or setting things as done (I don't remmember them all).
