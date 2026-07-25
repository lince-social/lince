A simple Kanban on the surface, with enough features to be used in a work environment.

Cards can be shown minimally or with a lot of information about them displayed.

The inside of cards, in a focused view will give away the ones assigned to the Record (task), the time spent on the task and more. The body of the Record has a lot of slash '/' commands to put nice enhanced Markdown parts.

One can do full CRUD of Records in the main view, looking at the columns. That means selecting a card and deleting it, changing the body by clicking on it to write in it or to check a box. Slash commands on the big picture view (all columns)?

I can sort and filter cards. Filtering will change the View (SQL) I am using to get data, sorting too.

# Basic CRUD
The columns of the Kanban dictate what quantity the Records have underneath. The user doesnt have to know that column Done is for quantities 1 by default. But they have to know if they want to change it.
- [ ] Moving one card from one column to the other issues an update on the quantity of the Record.
- [ ] I have a place to configure my columns, in that case i need to select one quantity for the the column, so Records with that quantity are shown in the respective column (would be cool to select a range, like from 1 to 2, 3 to 10). I must be able to sort them with drag and drop to say that column X is to be -1 and move it to -2. I must be able to click buttons to create new columns, give a name and type a quantity. Maybe have a tooltip to signal the reason behind using quantity.
- [ ] Have a way to CRUD column presets, like instead of Todo, WIP, Done its Backlog, Next, WIP, Finished. And i can apply one to this Kanban.
- [ ] Being able to select an existing view to use in the Kanban or create a new one, passing filters for category (at least). That will create a special Kanban View with the name given to it. This needs to be a Kanban View because we need to get a lot of different data that is specific to Kanban, if it's not the Kanban knowing how to create the View it is the backend, being polluted with a frontend implementation. 
- [ ] Having a small indicative that the connection with the backend is ok, can be used to signal that an update is taking place and when it is finished (maybe a cute little ball with different colors for the states - duds).
- [ ] Have a way to create a new Record, filling head, body, quantity, but also possible metadata.
- [ ] Be able to select one or more Records, to move them to another column or to delete them.

# Metadata

I am supposed to see this information on the expanded view of the card. As if the whole component, or half of it was this card, as if i entered it. Some of this information we need to show on the big picture of the Kanban, with all the cards, like the assignee, and parent, those are easy to understand the need to see. Others we might have to wait and see if they are needed.

- [ ] Date for the supposed start and end of the task.
- [ ] Time estimate, how much time do i think this is going to take, in hours and minutes (the data saved is in minutes).
- [ ] Play/Pause button to log time spent in the task. Play starts a work log, Pause ends one, time is added on Pause. Also we need to be able to full CRUD this so that if i spent some time before I can add it, if I inputted something wrong i can update the existing or delete it. 
- [ ] Assign the task to someone, by their name or username.
- [ ] Be able to set the parent/children of this task.
- [ ] Being able to set comments to the task, with pictures.
