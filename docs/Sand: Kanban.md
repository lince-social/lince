The Kanban sand when ready will be able to provide teams the organization necessary to tackle projects together in a classic way. The data they CRUD in Kanban is accessible in other sands to fit greater workflows though.

- [x] Have a way to create a new Record.
- [x] Moving one card from one column to the other issues an update on the quantity of the Record.
- [x] Cards can be shown minimally or with a lot of information about them displayed.
- [x] The columns of the Kanban dictate what quantity the Records have underneath. The user doesnt have to know that column Done is for quantities 1 by default. But they have to know if they want to change it. I have a place to configure my columns, in that case i need to select one quantity for the the column, so Records with that quantity are shown in the respective column (would be cool to select a range, like from 1 to 2, 3 to 10). I must be able to sort them with drag and drop to say that column X is to be -1 and move it to -2. I must be able to click buttons to create new columns, give a name and type a quantity. Maybe have a tooltip to signal the reason behind using quantity.
  - [x] Have a way to CRUD column presets, like instead of Todo, WIP, Done its Backlog, Next, WIP, Finished. And i can apply one to this Kanban.
- [x] Having a small indicative that the connection with the backend is ok, can be used to signal that an update is taking place and when it is finished (maybe a cute little ball with different colors for the states - duds).
- [ ] Be able to select one or more Records, to execute possible actions: move to another column, delete.
- [ ] Currently, metadata of Records is only visible and editable in the Record sand, a reusable sand for editing in-depth info about Records. We must be able to see such metadata, even if we can only interact with it through Record sand. Either way, here are the tasks for metadata control someway:
  - [ ] Date for the supposed start and end of the task.
  - [ ] Time estimate, how much time do i think this is going to take, in hours and minutes (the data saved is in minutes).
  - [ ] Play/Pause button to log time spent in the task. Play starts a work log, Pause ends one, time is added on Pause. Also we need to be able to full CRUD this so that if i spent some time before I can add it, if I inputted something wrong i can update the existing or delete it. 
  - [x] Assign the task to someone, by their name or username.
  - [ ] Be able to set the parent/children of this task.
  - [ ] Being able to CRUD threads and messages as links of records (that belong to a record) that can have the same complexity of body content: text, images...
  - [ ] The interaction with the body of Record must be able to have slash '/' commands to put add content in an easy way: typing /h3 will give you ### which is the end result that remains in the body (###). If we can make the body of a kanban have text, why not make it have the full editing and visualization that the 'Record' sand has for the body of the Record? We implemented the same checkbox clicking in the body of the record in kanban, why not put the body of the Record of the 'Record' sand?
    - [ ] Changing the body of Records in Kanban cards by clicking on it to write in it or to check a box (slash commands only in Record sand? too hard to implement such feature twice? gotta be a way)
