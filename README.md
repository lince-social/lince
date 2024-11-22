<p align=center>
<img width=24% src="https://github.com/lince-social/.github/blob/main/media/logo/preto_no_branco.png">
<img width=24% src="https://github.com/lince-social/.github/blob/main/media/logo/branco_no_preto.png">
<img width=24% src="https://github.com/lince-social/.github/blob/main/media/logo/preto_no_branco.png">
<img width=24% src="https://github.com/lince-social/.github/blob/main/media/logo/branco_no_preto.png">
</p>

# Lince
### Tool for registry, interconnection and automation of Needs and Contributions with open scope.
---

This is a [Next.js](https://nextjs.org) project bootstrapped with [`create-next-app`](https://nextjs.org/docs/app/api-reference/cli/create-next-app).

## Getting Started

First, run the development server:

```bash
npm run dev
# or
yarn dev
# or
pnpm dev
# or
bun dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

You can start editing the page by modifying `app/page.tsx`. The page auto-updates as you edit the file.

This project uses [`next/font`](https://nextjs.org/docs/app/building-your-application/optimizing/fonts) to automatically optimize and load [Geist](https://vercel.com/font), a new font family for Vercel.

## Learn More

To learn more about Next.js, take a look at the following resources:

- [Next.js Documentation](https://nextjs.org/docs) - learn about Next.js features and API.
- [Learn Next.js](https://nextjs.org/learn) - an interactive Next.js tutorial.

You can check out [the Next.js GitHub repository](https://github.com/vercel/next.js) - your feedback and contributions are welcome!

## Deploy on Vercel

The easiest way to deploy your Next.js app is to use the [Vercel Platform](https://vercel.com/new?utm_medium=default-template&filter=next.js&utm_source=create-next-app&utm_campaign=create-next-app-readme) from the creators of Next.js.

Check out our [Next.js deployment documentation](https://nextjs.org/docs/app/building-your-application/deploying) for more details.

## Installation:
Install [nix](https://nixos.org/download), then clone the repo through one of the following, maybe:

1:

https://github.com/lince-social/lince/archive/refs/heads/main.zip

2:
```bash
git clone https://github.com/lince-social/lince.git
```

3:
```bash
git clone git@github.com:lince-social/lince.git
``` 
To run lince, type on the directory you cloned it:

```bash
nix-shell
```

Or anywhere on your system:
```bash
nix-shell /path/to/lince
```
Then open your browser on the address:
```bash
localhost:5000
```

## Disclamer
This project is licensed under the GNU GPLv3 license. Crowdfunding is the source of development compensation:

[GitHub Sponsors](https://github.com/sponsors/lince-social) | [Patreon](https://www.patreon.com/lince_social) | [Apoia.se](https://www.apoia.se/lince) 

Lince tries to facilitate and automate the connection between people and resources, by transforming needs and contributions into data. The gains and losses related to the interaction, such as transportation, production and services themselves, remain the responsibility of the parties involved.

# DOCUMENTATION

@: What is Lince?

%: Lince is a tool for registry, interconnection and automation of Needs and Contributions with open scope. 

@: Ok, but what is it?

%: Lince is an app that can be used to model and/or automate personal tasks, items, computer actions, economic trades between parties... The limit is your imagination (and your wizard skills with computers). 

@: Sure...

%: Look, I'll explain everything in detail, so follow me. I promise the journey is worth the end, traveler.

---

Lince works with a PostgreSQL database, some answers about data types can be found there, it searches for it's data in ~/.config/lince/lince.sql, if not found there, it defaults to the directory Lince was cloned to, in src/db/versions/lince.sql. It is recomended to frequently backup the lince.sql file, if some error or mistake happens, your information is safe.

It is tempting to say TCITD, or 'The code is the documentation', because documentations are often not up to date and that is the best way of seeing the truth about the program. But that is not best for everyone, so this documentation in text was made. With that said, if you want to learn more about lince by reading the documentation, it is advised to also have the database declaration open: https://github.com/lince-social/lince/blob/main/src/db/schema.sql, for the documentation below has the name and data type of the tables, but not it's constraints.

Firstly the tables of the database will be explained, then the ways in which they can interact with themselves and/or your/other computers:


# TABLES

## Table: record

| record   |
| ---------|
| id       |
| quantity |
| head     |
| body     |
| location |

Lince is centered on the 'record' table, but like, according to the creator... like, what do they know?

Let's assume this is Lince's sun, it is the most capable one on the task of giving Lince life. The thing the app revolves around.

---

| record   | DATA STRUCTURE |
| ---------|----------------|
| id       | SERIAL         |
| quantity | REAL           |
| head     | TEXT           |
| body     | TEXT           |
| location | POINT          |

'id's are automatically generated.

'quantity' represents the availability of the record, if negative it is a Necessity, if positive, a Contribution, zero makes it not mean much, sometimes.

'head' and 'body' are meant to be parts of a whole, where one can be used for a short summary and the other a description, or one has all the information and the other holds tags for filtering through views. With a pubsub protocol, one can send a short information of the record, in this case it can be the head, and put the rest in the body. Only those interested in the head will ask for the body of the record. That way the minimum amount of information is sent over the network, making it faster and stuff, I think. 

'location' is an important information for interactions outside of computers (they exist, it's insane) or any other use you want to give it.

---

| record   | DATA STRUCTURE | USER INPUT |
| ---------|----------------|------------|
| id       | SERIAL         |            |
| quantity | REAL           | -1         |
| head     | TEXT           | Eat Apple  |
| body     | TEXT           |            |
| location | POINT          |            |

So, for an example, imagine that you like apples and you want to create a task to eat it today.

You create a 'record', giving it '-1' to the 'quantity', for that action is a Necessity in your life right now, and 'Eat Apple' to the 'head'.

---

| record   | DATA STRUCTURE | USER INPUT | ACTUAL RECORD |
| ---------|----------------| -----------| --------------|
| id       | SERIAL         |            | 1             |
| quantity | REAL           | -1         | -1            |
| head     | TEXT           | Eat Apple  | Eat Apple     |
| body     | TEXT           |            | NULL          |
| location | POINT          |            | NULL          |

The end result, on the PostgreSQL database, is this record. In summary, fields with 'NOT NULL' that have defaults don't need to be filled, as it happens automatically.

---

| id  | quantity | head      | body | location |
|-----|----------|-----------|------|----------|
| 1   | -1       | Eat Apple | NULL | NULL     |


The theoretical apple eater in the example chose to put -1 in 'quantity' because they have a view that gives them all the records with a negative 'quantity' (quantity < 0).

So they will see the 'Eat Apple' task on that view, but more about that in a second, now look at other examples of records (rows are set horizontally now).

---

| id  | quantity | head        | body                 | location |
|-----|----------|-------------|----------------------|----------|
| 1   | -1       | Eat Apple   | NULL                 | NULL     |
| 2   | 1        | Apple       | Item, Food           | NULL     |
| 3   | 0        | Brush Teeth | Action, Hygiene      | NULL     |
| 4   | 3        | Toothbrush  | Item, Hygiene        | NULL     |
| 5   | -1       | Meditate    | Action, Spirituality | NULL     |

As you can see, there are records with different quantities, heads, and bodies. They are modeling actions and items.

The user likes to center it's filtering through the body column, seeing all actions, or all items of a certain area of their lives, like Hygiene, each in different created views.

------------------------------------------------------------------------------------------------------------------

## Table: views

| views     | DATA STRUCTURE |
|-----------|----------------|
| id        | SERIAL         |
| view      | TEXT           |
| view_name | TEXT           |

We've spoken so much about views, let's dive into them. They are essentially SQL queries, allowing you to select what columns you want to see, filtered, ordered and much more, just the way you want it.
The view column has the SQL query, the view_name has the view's name. Simple, right?

------------------------------------------------------------------------------------------------------------------

## Table: `configuration`

| configuration           | DATA STRUCTURE |
|-------------------------|----------------|
| id                      | SERIAL         |
| quantity                | REAL           |
| save_mode               | VARCHAR        |
| view_id                 | INTEGER        |
| column_information_mode | VARCHAR        |
| keymap                  | JSONB          |
| truncation              | JSONB          |
| table_query             | JSONB          |
| language                | VARCHAR        |
| timezone                | VARCHAR        |

This table is responsible for changing the behavior of Lince. The 'quantity' sets the active configuration, with the value of 1, the other rows have it 0. 'save_mode' can be automatic or manual, happening only when the user demands it, or after every database change. 'view_id' sets the rows you will see, this is a reference to the id of a row in view. 'column_information_mode' can be 'silent for no information, 'short', for the data types and constraints or 'verbose' for the previous information plus a description of the functionality/usage of the column. Keymap is for changing the commands sent manually (TODO) so 'd' is not for deleting records but for 'd'escribing lince, printing it's documentation. 'truncation' is somewhat deprecated, used for truncating cells's contents. 'table_query' is too deprecated, somewhat, from the terminal days, but essentially it changes how a table is queried. 'language' and 'timezone' are for the language and timezone :).

------------------------------------------------------------------------------------------------------------------

## Table: `history`

| history      | DATA STRUCTURE |
|--------------|----------------|
| id           | SERIAL         |
| record_id    | INTEGER        |
| change_time  | TIMESTAMP      |
| old_quantity | REAL           |
| new_quantity | REAL           |

History is a table that automatically logs the change of a record's quantity, to use the 'sum' table.

------------------------------------------------------------------------------------------------------------------

## Table: `karma`

| karma      | DATA STRUCTURE |
|------------|----------------|
| id         | SERIAL         |
| quantity   | INTEGER        |
| expression | TEXT           |

Karma has it's own section further below for a deeper understanding, it's a way of combining data and taking actions automatically. The 'expression' column contains lince-python-like logic.

------------------------------------------------------------------------------------------------------------------

## Table: `frequency`

| frequency    | DATA STRUCTURE |
|--------------|----------------|
| id           | SERIAL         |
| quantity     | INTEGER        |
| day_week     | REAL           |
| months       | REAL           |
| days         | REAL           |
| seconds      | REAL           |
| next_date    | TIMESTAMP      |
| finish_date  | DATE           |
| catch_up_sum | INTEGER        |

'frequency' contains a plethora of ways for modeling a frequency, this is a table that makes more sense when one understands karma. If one wants to automate something that happens, changes every 4 days 9 hours and 53 seconds, finishing at 2029, it can be done so. 'day_week' is the day of the week, monday is 1. 'months', 'days' and 'seconds' are what they seem, 'next_date' is the column that changes when the time comes, advancing to a next date in accordance with the previous columns. 'finish_date' asserts that it will only run before a certain date, 'catch_up_sum' is a value that when negative and the 'next_date' is way behind the current date, the returned value will be of a certain amount, so instead of returning one, it will return the absolute value of this column. Example:

There is one frequency for every 60 seconds, starting now, but I have activated it with karma 10 minutes from now. With a 'catch_up_sum' of -2 it will return 2, it should return 10, for it's been 10 minutes stopped, and then return 1 every 1 minute, but with a 'catch_up_sum' of -2 it returns 2. If the 'catch_up_sum' is more than zero, it will multiply the value returned, so if instead of -2 it's '2.5, after 10 minutes from being stopped it will return 25, and 2.5 every subsequent minute.

------------------------------------------------------------------------------------------------------------------

## Table: `sum`

| sum               | DATA STRUCTURE |
|-------------------|----------------|
| id                | SERIAL         |
| quantity          | INTEGER        |
| record_id         | INTEGER        |
| interval_relative | BOOL           |
| interval_length   | INTERVAL       |
| sum_mode          | INTEGER        |
| end_lag           | INTERVAL       |
| end_date          | TIMESTAMP      |

This table is responsible for returning a sum of change. Change of a record's quantity over time. You have the 'record_id' to know what record to look for.
'interval_length' is a period, can be 1 day, 8 months, etc. If 'interval_relative' is TRUE, will sum all the changes ending now, starting at the past based on the 'interval_length' (ex: 'interval_length' of 8 months and 'interval_relative' is TRUE will make it look at the past 8 months). If FALSE an 'end_date' will need to be supplied. so the past 8 months ending in a certain date. 'end_lag' will give a space between now and the end point of a relative interval. So if the 'interval_length' is 8 months and 'interval_relative' is TRUE, an 'end_lag' of 2 months will make the sum look at the 2 months back to 10 months back, starting 10 months ago, ending 2 months ago. 'sum_mode' tells Lince what to sum, if 0 it's a delta, essentially doing a quantity now - quantity earlier. If it's negative, it sums all negative changes, positive, idem.


------------------------------------------------------------------------------------------------------------------

## Table: `command`

| command  | DATA STRUCTURE |
|----------|----------------|
| id       | SERIAL         |
| quantity | INTEGER        |
| command  | TEXT           |

'command' is a command you can run in through a shell, like 'python app.py' or 'touch grass.txt'.

------------------------------------------------------------------------------------------------------------------

## Table: `transfer`

| transfer                          | DATA STRUCTURE |
|-----------------------------------|----------------|
| id                                | SERIAL         |
| records_received                  | JSON           |
| records_contributed               | JSON           |
| agreement                         | JSON           |
| agreement_time                    | TIMESTAMP      |
| transfer_confirmation             | JSON           |
| transfer_time                     | TIMESTAMP      |

**WORK IN PROGRESS**
'records_received' is a collection of records and their quantities that will interact with our records, things you will receive. 'records_contributed' are the records you will contribute and their quantities, to the records of other parties, can be more than one party. So you can receive 5 moneys and an apple for driving someone from A to B. You don't work with it, but you have a car and their destination was on the way of yours. 'agreement' is a collection of agreement by all parties involved. 'agreement_time' is the moment every party agreed for the conditions of the trade, who will receive what. 'transfer_confirmation' is also a collection but with a confirmation from all parties that the transfer was successful, and 'transfer_time' for saving the event's moment.

---

# KARMA

The 'expression' column in the 'karma' table is like Lince's brain.

It is divided into two parts left and right, like a brain, separated by a '='. For karma to have it's effect, a karma() function is called (currently every 60 seconds), in each expression first the right side has some information searched and replaced if necessary, then it is evaluated, and passed to the left side. (What?)

Let's look at an example, say you have a record:

| record   | VALUE        |
|----------|--------------|
| id       | 1            |
| quantity | 0            |
| head     | Eat Apple    |
| body     | Action, Food |
| location |              |

If we pull the Record Quantity (rq) of this record we get zero (0). In other words, if we pull the rq1, the Record Quantity of the record with ID 1, we get zero (0).

Our karma expression can be a simple:
| karma      | VALUE    |
|------------|----------|
| id         | 1        |
| quantity   | 1        |
| expression | rq1 = -1 |

For demonstrating purposes, karma expressions will be shown like this, no need for the full table:
```lince
rq1 = -1
```

When the karma() function is called, every row will have it's consequence if needed. In this case the consequence is that the quantity in the record with ID 1 is now -1.

How does that happen? The right side of the expression, in this case '-1' is evaluated, a python eval(). The right side is now literal python, you can declare classes, pull libraries, write everything, including writing just -1. The left side of the expression, since it has only the mention of a record quantity will receive that number without problems, the quantity in the record with ID 1 is now -1.

------------------------------------------------------------------------------------------------------------------

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
|----------|--------------|
| id       | 1            |
| quantity | -1           |
| head     | Eat Apple    |
| body     | Action, Food |
| location |              |

------------------------------------------------------------------------------------------------------------------

# The Zero Case:

And what if the rq1 was 1?
In that case, it would still be one, why? Because when the right side is zero, nothing changes on the left side. That is not a bug, but a feature, that way, expressions can be ignored if they dont meet certain conditions:
```lince
rq1 = -1 * (rq1 == 10)
```
In this case the '(rq1 == 10)' in python will be set to False, because '(1 == 10)' is False, and when '-1 * False' is read by python it returns zero. So the record quantity will remain 1.

But what if I want it to be 0? Then you add '*' before the '=':
```lince
rq1 *= -1 * (rq1 == 10)
```
If rq1 is ten (10), it turns into one (1), if it is not ten (10), then it turns into zero (0).

------------------------------------------------------------------------------------------------------------------

# How it works, really:

A karma expression is a mix of python and regex for searching and replacing, but that is not all it can do. One of the features is to be able to run commands located in the command table.

Let's say you have a command:

| command  | VALUE           |
|----------|-----------------|
| id       | 3               |
| quantity | 1               |
| command  | touch grass.txt | <-- touch <file> in Linux creates the file.

This command with ID 3 can be referenced in a karma expression with 'c3'.

If we have a record:

| comman   | VALUE            |
|----------|------------------|
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

------------------------------------------------------------------------------------------------------------------

# Automation, real automation:

But what if we want to automate everything? Like literally everything.

For that, a general enough system needs to exist. When we talk automation we talk about labor automation, time that does not need to be spent doing something. Automation is a tool, not an end, and it can be a tool for the benefit of everyone, by reducing the labor needed to satiate human needs.

Lince aims to be a tool that can model human needs and contributions to meet those needs, automating as much as possible for that goal. And the biggest source of automation in Lince, is not shortening the time it takes to give the computer an instruction, but removing the need to do it, satiating that need.

The 'frequency' table allows for rows with different frequencies to return 1 to a karma expression, activating scripts changing the values in certain cells in certain tables, let's see it in action:

| frequency    | VALUE                     |
|--------------|---------------------------|
| id           | 6                         |
| quantity     | 1                         |
| day_week     |                           |
| months       |                           |
| days         | 1                         |
| seconds      |                           |
| next_date    | 2024-01-01 22:00:00+00:00 |
| finish_date  |                           |
| when_done    |                           |

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
|----------|------------------------------|
| id       | 7                            |
| quantity | 1                            |
| command  | python temperature_getter.py |

| record   | VALUE            |
|----------|------------------|
| id       | 10               |
| quantity | 0                |
| head     | Get a sweater    |
| body     | Action, Clothes  |
| location |                  |


This pseudo python script uses a weather API to get the tomorrow's lowest temperature (in Celsius, obviously) in a certain city, it then prints it to the terminal, very UNIX-like. If we have that, we can ask Lince to put that temperature inside a karma expression and use it.

```lince
rq10 = -f6 * (co7 <= 15)
```
Notice it says 'co7', there is a letter 'o' because it's supposed to be run a command that outputs the result to a file, so lince can read it. After that, by reusing a previously created frequency, we can create an expression that makes the quantity of the record with ID 10 be -1 at 22h of everyday, if the command with ID 7 returns tommorow's lowest temperature as a value equal to or less then 15. If we have a view that shows only records with a negative quantity, a person will only see the necessity of packing a sweater when tomorrow is 'cold' enough.

If we combine several parenteses and multiply them, make sums and output the right side's computation to the left one, we basically have with the right parameters a trained machine learning model in a single karma expression line, not ergonomic at all, but cool; better to put it in a file.

When opening the browser version you will see a view and this table:

| App                 | Operations   | Tables            |
|---                  |---           |---                |
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

------------------------------------------------------------------------------------------------------------------

# Personal Note & Tips:
Modeling all items and actions to perform takes a while. Making a Lince instance, understanding the possibilities is to me a long term effort, ever increasing the value you get from computers. The possibilites are vast, limited by one's domain of computers and imagination, when combined, wizardry skills arise.

From a usage standpoint, I recommend having several views, one for each table and a personal view for tasks, mine is:
```sql
SELECT id, head FROM record WHERE quantity < 0 or LOWER(body) IS NULL
```

I also use the 'body' column in 'record' table as a tag holder, so every area in my life has either 'Action' or 'Item' and 'AREA'. With that, I have a view for every area and change configs rapidly with a2, a4, a3 so I can view the commands for running things or setting things as done (I don't remmember them all).
