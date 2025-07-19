# Record

| record   |
| -------- |
| id       |
| quantity |
| head     |
| body     |
| location |

Lince is centered on the 'record' table, but like, according to the creator... like... what does he know?

---

| record   | DATA TYPE                 |
| -------- | ------------------------- |
| id       | INTEGER                   |
| quantity | FLOAT                     |
| head     | TEXT                      |
| body     | TEXT                      |
| location | 3D POINT (still thinking) |

'id's are automatically generated.

'quantity' represents the availability of the record, if negative it is a Necessity, if positive, a Contribution, zero makes it not mean much, sometimes.

'head' and 'body' are meant to be parts of a whole, where one can be used for a short summary and the other a description, or one has all the information and the other holds tags for filtering through views. With a pubsub protocol, one can send a short information of the record, in this case it can be the head, and put the rest in the body. Only those interested in the head will ask for the body of the record. That way the minimum amount of information is sent over the network, making it faster and stuff, I think.

'location' is an important information for interactions outside of computers (they exist, it's insane) or any other use you want to give it.

---

| record   | DATA TYPE | USER INPUT |
| -------- | --------- | ---------- |
| id       | INTEGER   |            |
| quantity | FLOAT     | -1         |
| head     | TEXT      | Eat Apple  |
| body     | TEXT      |            |
| location | POINT     |            |

So, for an example, imagine that you like apples and you want to create a task to eat it today.

You create a 'record', giving it '-1' to the 'quantity', for that action is a Necessity in your life right now, and 'Eat Apple' to the 'head'.

---

| record   | DATA TYPE | USER INPUT | ACTUAL RECORD |
| -------- | --------- | ---------- | ------------- |
| id       | INTEGER   |            | 1             |
| quantity | FLOAT     | -1         | -1            |
| head     | TEXT      | Eat Apple  | Eat Apple     |
| body     | TEXT      |            | NULL          |
| location | POINT     |            | NULL          |

The end result, on the database, is this record.

---

Here is an example of different possible records for individual items and actions.

| id  | quantity | head        | body            | location |
| --- | -------- | ----------- | --------------- | -------- |
| 1   | -1       | Eat Apple   | NULL            | NULL     |
| 2   | -1       | Apple       | Item, Food      | NULL     |
| 3   | 0        | Brush Teeth | Action, Hygiene | NULL     |
| 4   | 3        | Toothbrush  | Item, Hygiene   | NULL     |
| 5   | -1       | Meditate    | Action          | NULL     |
