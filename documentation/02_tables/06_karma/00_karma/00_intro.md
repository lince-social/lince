# Karma

| karma      | DATA STRUCTURE |
| ---------- | -------------- |
| id         | SERIAL         |
| quantity   | INTEGER        |
| expression | TEXT           |

Karma has it's own section further below for a deeper understanding, it's a way of combining data and taking actions automatically. The 'expression' column contains lince-python-like logic.

# KARMA

The 'expression' column in the 'karma' table is like Lince's brain.

It is divided into two parts left and right, like a brain, separated by a '='. For karma to have it's effect, a karma() function is called (currently every 60 seconds), in each expression first the right side has some information searched and replaced if necessary, then it is evaluated, and passed to the left side. (What?)

Let's look at an example, say you have a record:

| record   | VALUE        |
| -------- | ------------ |
| id       | 1            |
| quantity | 0            |
| head     | Eat Apple    |
| body     | Action, Food |
| location |              |

If we pull the Record Quantity (rq) of this record we get zero (0). In other words, if we pull the rq1, the Record Quantity of the record with ID 1, we get zero (0).

Our karma expression can be a simple:
| karma | VALUE |
|------------|----------|
| id | 1 |
| quantity | 1 |
| expression | rq1 = -1 |

For demonstrating purposes, karma expressions will be shown like this, no need for the full table:

```lince
rq1 = -1
```

When the karma() function is called, every row will have it's consequence if needed. In this case the consequence is that the quantity in the record with ID 1 is now -1.

How does that happen? The right side of the expression, in this case '-1' is evaluated, a python eval(). The right side is now literal python, you can declare classes, pull libraries, write everything, including writing just -1. The left side of the expression, since it has only the mention of a record quantity will receive that number without problems, the quantity in the record with ID 1 is now -1.

---

# Simple Example:

Now let's say that the rq1 is still zero (0), another karma expression could be:

```lince
rq1 = rq1 -1
```

So now everytime karma() is called, it will diminish the quantity by one, insead of always setting it to -1. The rq1 is searched and replaced with it's actual value:

```lince
rq1 = rq1 -1

      |
      V

rq1 = 0 -1

       |
       V

rq1 = -1
```

Now your rq1 is -1:

| record   | VALUE        |
| -------- | ------------ |
| id       | 1            |
| quantity | -1           |
| head     | Eat Apple    |
| body     | Action, Food |
| location |              |

---

# The Zero Case:

And what if the rq1 was 1?
In that case, it would still be one, why? Because when the right side is zero, nothing changes on the left side. That is not a bug, but a feature, that way, expressions can be ignored if they dont meet certain conditions:

```lince
rq1 = -1 * (rq1 == 10)
```

In this case the '(rq1 == 10)' in python will be set to False, because '(1 == 10)' is False, and when '-1 \* False' is read by python it returns zero. So the record quantity will remain 1.

But what if I want it to be 0? Then you add '\*' before the '=':

```lince
rq1 *= -1 * (rq1 == 10)
```

If rq1 is ten (10), it turns into one (1), if it is not ten (10), then it turns into zero (0).

---

# How it works, really:

A karma expression is a mix of python and regex for searching and replacing, but that is not all it can do. One of the features is to be able to run commands located in the command table.

Let's say you have a command:

| command  | VALUE           |
| -------- | --------------- | ------------------------------------------- |
| id       | 3               |
| quantity | 1               |
| command  | touch grass.txt | <-- touch <file> in Linux creates the file. |

This command with ID 3 can be referenced in a karma expression with 'c3'.

If we have a record:

| comman   | VALUE            |
| -------- | ---------------- |
| id       | 2                |
| quantity | 1                |
| head     | Create grass.txt |
| body     | Action, Command  |
| location |                  |

We can create a semi-automation, reducing the steps, the clicks, the button presses, the time it takes to do that job:

```lince
rq2,c3 = (rq2 == 0) * 1
```

When the quantity in record with ID 2 is 0, the command with ID 3 is run through the computer's shell and the grass.txt is created.

---

# Automation, real automation:

But what if we want to automate everything? Like literally everything.

For that, a general enough system needs to exist. When we talk automation we talk about labor automation, time that does not need to be spent doing something. Automation is a tool, not an end, and it can be a tool for the benefit of everyone, by reducing the labor needed to satiate human needs.

Lince aims to be a tool that can model human needs and contributions to meet those needs, automating as much as possible for that goal. And the biggest source of automation in Lince, is not shortening the time it takes to give the computer an instruction, but removing the need to do it, satiating that need.

The 'frequency' table allows for rows with different frequencies to return 1 to a karma expression, activating scripts changing the values in certain cells in certain tables, let's see it in action:

| frequency   | VALUE                     |
| ----------- | ------------------------- |
| id          | 6                         |
| quantity    | 1                         |
| day_week    |                           |
| months      |                           |
| days        | 1                         |
| seconds     |                           |
| next_date   | 2024-01-01 22:00:00+00:00 |
| finish_date |                           |
| when_done   |                           |

Let's say that we are in 2024-01-01 21:00:00 living in a place with a +0 timezone and have a karma expression:

```lince
c3 = f6

     |
     V

c3 = 0
```

In that same day, when ten (10) P.M. rolls around, the time is exactly '2024-01-02 22:00:00+00:00' or after, the karma() function will receive not zero (0), but one (1) from the frequency with ID 6:

```lince
c3 = 1
```

The command will receive one, it will be run, the 'touch grass.tx' will be executed, the grass.txt file will be created. It achieves that by updating the date in next_date by one day, as set by the day column with value one (1) in this case.

In the TODO list, ignore if you want: make all the expressions that use a certain frequency be checked and run if needed before updating next_date. Right now one activation of a frequency negates all other expressions that need to return 1 at that frequency, because it will already have been activated and updated.

---

Until now the command was only at the left side, as something that was run because of a consequence on the right side. But, it can also be run and it's output passed as a value from the right side to the left one:

| command  | VALUE                        |
| -------- | ---------------------------- |
| id       | 7                            |
| quantity | 1                            |
| command  | python temperature_getter.py |

| record   | VALUE           |
| -------- | --------------- |
| id       | 10              |
| quantity | 0               |
| head     | Get a sweater   |
| body     | Action, Clothes |
| location |                 |

This pseudo python script uses a weather API to get the tomorrow's lowest temperature (in Celsius, obviously) in a certain city, it then prints it to the terminal, very UNIX-like. If we have that, we can ask Lince to put that temperature inside a karma expression and use it.

```lince
rq10 = -f6 * (co7 <= 15)
```

Notice it says 'co7', there is a letter 'o' because it's supposed to be run a command that outputs the result to a file, so lince can read it. After that, by reusing a previously created frequency, we can create an expression that makes the quantity of the record with ID 10 be -1 at 22h of everyday, if the command with ID 7 returns tommorow's lowest temperature as a value equal to or less then 15. If we have a view that shows only records with a negative quantity, a person will only see the necessity of packing a sweater when tomorrow is 'cold' enough.

---

When opening the browser version you will see a view and this table:

| App                 | Operations   | Tables            |
| ------------------- | ------------ | ----------------- |
| [E] Exit            | [C] Create   | [0] Configuration |
| [H] Help            | [R] Read     | [1] History       |
| [S] Save DB         | [U] Update   | [2] Record        |
| [L] Load DB         | [D] Delete   | [3] Karma         |
| [A] Activate Config | [Q] Query    | [4] Frequency     |
|                     | [F] SQL File | [5] Command       |
|                     |              | [6] Sum           |
|                     |              | [7] Transfer      |
|                     |              | [8] View          |

As you can see, this menu has letters and numbers. All the numbers in the 'Tables' column can be combined with the CRUD operations in the 'Operations' column. so if you want to create, delete, update, or read a row you can input in the input box at the top of the screen the number of the table, and the operation, so deleting rows from the 'karma' table would be '3d'. If you type one of the crud operations without anumber, it defaults to the record table. Because Lince is still in development, the place for you to write what you want to delete is shown in the window you opened Lince, in the terminal that is. There you will see a python input() for the creation, deletion and updating of table rows. The read operation is also a work in progress, as it used to work in the terminal and is a sort of a 'Jerry-rig' or 'gambiarra'.

Just typing a number will make the record with ID of that number to have it's quantity set to zero.

Since the config has a view_id, you will create a view, and put it in as many configs as you want to. To change configs, and also the view, you write 'a' followed by a number, corresponding to the configuration's ID, so 'a2' will change to configuration 2; that configuration might have a 'view_id' of 30, so 'a2' will give me also the view of the records queried by the view with ID 30.

That's also something of the terminal days. There are plans of making buttons and stuff, but only because people think they need them, with the box up top for writting commands everything happens fast and you can keep you hands on the keyboard, the best way of using computers.
