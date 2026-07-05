Table sand is responsible for being the base of the components. Since original data in database is in a table the Table sand is the simplest to translate the incoming data to a visual structure.

Functionalities we Need:

 - [ ] Delete rows: That happens when he have the id column available. We need that column to understand what exactly needs to be deleted.
- [ ] Update: I can already click a cell or press F2 (its a standard, i dunno why) and edit the contents of the cell. The saving is automatic, it waits for 300ms of non editing to save the data.
- [ ] Create rows: We have an endpoint that gives us the writeable columns from every table. So currently, when we ask the Table sand to create data the component understands the View it is using and analyzing the SQL it knows the current table. Then we simply request the columns from the endpoint passing the table and currently the component opens a side panel with the fields one needs to write to create a new line in the current table. There's a dropdown to choose to create in other tables too. 
- [ ] Filters?