| Columns  | User Input | Actual Record | Data Type       |
|----------|------------|----------------|-----------------|
| Id       |            | 1              | Number (Int)    |
| Quantity | -1         | -1             | Number (Float)  |
| Head     | Eat Apple  | Eat Apple      | Text            |
| Body     |            |                | Text            |

All possible Needs/Contributions, be they habits, tasks, ideas, notes, items, goals... are put into Records. They are how we model everything in Lince. The rest of the features is just a way to interact with the Records, to change them, to act on the world based on the Record's state.

'id's are automatically generated.

Lince is build around the mental framework/philosphy that the 'quantity' represents the availability of the Record. If that quantity is negative, it is a Necessity, if positive, it is a Contribution, zero makes it not a Necessity and not a Contribution.

'head' can be thought of as a title and 'body' as a description.

So, for an example, imagine that you like apples and you want to create a task to eat it today. You create a Record, giving it '-1' to the 'quantity', for that action is a Necessity in your life right now, and 'Eat Apple' to the 'head'. The end result is the Record shown at the start.

Here is an example of different possible records for individual items and actions.

| Id | Quantity | Head        | Body            |
|----|----------|-------------|-----------------|
| 1  | -1       | Eat Apple   |                 |
| 2  | -1       | Apple       |       |
| 3  | -1       | Meditate    |           |
| 4 | -1 | Client Meeting | Remember to talk ab... |
| 5 | 0 | Class XYZ Notes | Introduction: The ... |