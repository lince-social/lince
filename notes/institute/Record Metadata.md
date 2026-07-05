Records also have more data associated with them. Only 'quantity', 'head', and 'body' would not be enough to model all Needs. What about cost, location and media properties? That's why we have Record Metadata.

There's 'category' which is used mostly as tags to filter: 'task, work, projectMapleSyrup'.

There's also the Extension table, with a 'freestyle_data_structure' property or 'fds' for short. It can store anything, it's whatever. In the end it's just another text field that could go into like the 'body' of a record but it would pollute it, and we have no way of controlling the version of that fds. You can put anything in there like json to make a chess game with the past moves and current state, the sky is the limit.

The extension is a quick fix to the problem of having a lot of different workflows. We need to think about what concepts are commonly used to make them become first class citizens in Lince, like the cost of stuff and location, if they are simply text that we know the structure it is not as efficient to deal with and a security problem.

Different components in the Web Interface have further Metadata on Records, such as: logging time spent done something when using Records as tasks. Assignees to set what user of the Lince Organ will do the task, time estimate, start and end date for task...