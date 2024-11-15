# Lince Documentation

## What is Lince?
Lince is a tool for registry, interconnection and automation of Needs and Contributions with open scope. 

## Ok, but what is it?
Lince is an app that can be used to model and/or automate personal tasks, items, computer actions, economic trades between parties... The limit is your imagination (and your wizard skills with computers). 

## Sure...
I'll explain everything in detail, so follow me. I promisse the journey is worth the end, traveler.

---

# Lince Documentation

## General Functioning

### Table: record

| record |
| --- |
| id |
| quantity |
| head |
| body |
| location |

Lince is centered on the 'record' table, but like, according to the creator... like, what does he know?

Let's assume this is Lince's sun, it is the most capable one on the task of giving Lince life. The thing the app revolves around.

---

# Lince Documentation

## General Functioning

### Table: record

| record | DATA STRUCTURE |
| --- | --- |
| id | SERIAL PRIMARY KEY |
| quantity | REAL NOT NULL DEFAULT 1 |
| head | TEXT             |
| body | TEXT             |
| location | POINT        |

'id's are automatically generated.

'quantity' represents the availability of the record, if negative it is a Necessity, if positive, a Contribution, zero makes it not mean much, sometimes.

'head' and 'body' are meant to be parts of a whole, where one can be used for a short summary and the other a description, or one has all the information and the other holds tags for filtering through views. With a pubsub protocol, one can send a short information of the record, in this case it can be the head, and put the rest in the body. Only those interested in the head will ask for the body of the record. That way the minimum amount of information is sent over the network, making it faster and stuff, I think.

'location' is an important information for interactions outside of computers (they exist, it's insane) or any other use you want to give it.

---

# Lince Documentation

## General Functioning

### Table: record

| record | DATA STRUCTURE | USER INPUT |
| --- | --- | --- |
| id | SERIAL PRIMARY KEY |  |
| quantity | REAL NOT NULL DEFAULT 1 | -1 |
| head | TEXT             | Eat Apple |
| body | TEXT             |  |
| location | POINT        |  |

So for an example imagine that you like apples and you want to create a task to eat it today.

You create a 'record', giving it '-1' to the 'quantity', for that action is a Necessity in your life right now, and 'Eat Apple' to the 'head'.

---

# Lince Documentation

## General Functioning

### Table: record

| record | DATA STRUCTURE | USER INPUT | ACTUAL RECORD |
| --- | --- | --- | --- |
| id | SERIAL PRIMARY KEY |  | 1 |
| quantity | REAL NOT NULL DEFAULT 1 | -1 | -1 |
| head | TEXT             | Eat Apple | Eat Apple |
| body | TEXT             |  | NULL |
| location | POINT        |  | NULL |

The end result, on the PostgreSQL database, is this record. In summary, fields with 'NOT NULL' that have defaults don't need to be filled, as it happens automatically.

---

# Lince Documentation

## General Functioning

### Table: record

| id | quantity | head | body | location |
| --- | --- | --- | --- | --- |
| 1| -1 | Eat Apple | NULL | NULL |
| | |  | | |
| | |  | | |
| | |  | | |
| | |  | | |

The theoretical apple eater in the example chose to put -1 in 'quantity' because they have a view that gives them all the records with a negative 'quantity' (quantity < 0).

So they will see the 'Eat Apple' task on that view, but more about that in a second, now look at other examples of records (rows are set horizontally now).

---

# Lince Documentation

## General Functioning

### Table: record

| id | quantity | head | body | location |
| --- | --- | --- | --- | --- |
| 1| -1 | Eat Apple | NULL | NULL |
| 2| 1| Apple | Item, Food | NULL |
| 3| 0| Brush Teeth | Action, Hygiene | NULL |
| 4| 3| Toothbrush | Item, Hygiene | NULL |
| 5| -1| Meditate | Action, Spirituality | NULL |

As you can see, there are records with different quantities, heads, and bodies. They are modeling actions and items.

The user likes to center it's filtering through the body column, seeing all actions, or all items of a certain area of their lives, like Hygiene, each in different created views.

---

# Lince Documentation

## General Functioning

### Table: views

| `views`               |
|-----------------------|
| id                    |
| view                  |
| view_name             |

We've spoken so much about views, let's dive into them. They are essentially SQL queries, allowing you to select what columns you want to see, filtered, ordered and much more, just the way you want it.

---
<!-- ``` -->
<!-- ~~~graph-easy --as=boxart -->
<!-- [ record ] - to -> [ B ] -->
<!-- ~~~ -->
<!-- ``` -->

<!-- ``` -->
<!-- ~~~graph-easy --as=boxart -->
<!-- [ A ] - to -> [ B ] -->
<!-- ~~~ -->
<!-- ``` -->
<!-- ### Table Structure -->

<!-- | `views`               | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | view TEXT NOT NULL DEFAULT 'SELECT * FROM record' | -->
<!-- | view_name TEXT        | -->

<!-- | `configuration`       | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | quantity REAL NOT NULL DEFAULT 1 | -->
<!-- | save_mode VARCHAR(9) NOT NULL DEFAULT 'Automatic' CHECK (save_mode in ('Automatic', 'Manual')) | -->
<!-- | view_id INTEGER NOT NULL DEFAULT 1 | -->
<!-- | column_information_mode VARCHAR(7) NOT NULL DEFAULT 'verbose' CHECK (column_information_mode in ('verbose', 'short', 'silent')) | -->
<!-- | keymap jsonb NOT NULL DEFAULT '{}' | -->
<!-- | truncation jsonb NOT NULL DEFAULT '{"head": 150, "body": 150, "view": 100, "command": 150}' | -->
<!-- | table_query jsonb NOT NULL DEFAULT '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC", "command": "SELECT * FROM command ORDER BY id ASC"}' | -->
<!-- | language VARCHAR(15) NOT NULL DEFAULT 'en-US' | -->
<!-- | timezone VARCHAR(3) NOT NULL DEFAULT '-3' | -->

<!-- | `record`              | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | quantity REAL NOT NULL DEFAULT 1 | -->
<!-- | head TEXT             | -->
<!-- | body TEXT             | -->
<!-- | location POINT        | -->

<!-- | `history`             | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | record_id INTEGER NOT NULL | -->
<!-- | change_time TIMESTAMP WITH TIME ZONE DEFAULT NOW() | -->
<!-- | old_quantity REAL NOT NULL | -->
<!-- | new_quantity REAL NOT NULL | -->

<!-- | `karma`               | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | quantity INTEGER NOT NULL DEFAULT 1 | -->
<!-- | expression TEXT       | -->

<!-- | `frequency`           | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | quantity INTEGER NOT NULL DEFAULT 1 | -->
<!-- | day_week REAL         | -->
<!-- | months REAL DEFAULT 0 NOT NULL | -->
<!-- | days REAL DEFAULT 0 NOT NULL | -->
<!-- | seconds REAL DEFAULT 0 NOT NULL | -->
<!-- | next_date TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL | -->
<!-- | finish_date DATE      | -->
<!-- | when_done INTEGER NOT NULL DEFAULT 0 | -->

<!-- | `sum`                 | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | quantity INTEGER NOT NULL DEFAULT 1 | -->
<!-- | record_id INTEGER NOT NULL | -->
<!-- | sum_mode INTEGER NOT NULL DEFAULT 0 CHECK (sum_mode in (-1, 0, 1)) | -->
<!-- | interval_mode VARCHAR(10) NOT NULL DEFAULT 'relative' CHECK (interval_mode IN ('fixed', 'relative')) | -->
<!-- | interval_length INTERVAL NOT NULL | -->
<!-- | end_lag INTERVAL       | -->
<!-- | end_date TIMESTAMP WITH TIME ZONE DEFAULT now() | -->

<!-- | `command`             | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | quantity INTEGER NOT NULL DEFAULT 1 | -->
<!-- | command TEXT NOT NULL | -->

<!-- | `transfer`            | -->
<!-- |-----------------------| -->
<!-- | id SERIAL PRIMARY KEY | -->
<!-- | records_received json | -->
<!-- | records_contributed json | -->
<!-- | receiving_agreement BOOL | -->
<!-- | contributing_agreement BOOL | -->
<!-- | agreement_time TIMESTAMP WITH TIME ZONE | -->
<!-- | receiving_transfer_confirmation BOOL | -->
<!-- | contributing_transfer_confirmation BOOL | -->
<!-- | transfer_time TIMESTAMP WITH TIME ZONE | -->

<!-- ### Table Columns Only -->


<!-- | `configuration`       | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | quantity              | -->
<!-- | save_mode             | -->
<!-- | view_id               | -->
<!-- | column_information_mode| -->
<!-- | keymap                | -->
<!-- | truncation            | -->
<!-- | table_query           | -->
<!-- | language              | -->
<!-- | timezone              | -->

<!-- | `record`              | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | quantity              | -->
<!-- | head                  | -->
<!-- | body                  | -->
<!-- | location              | -->

<!-- | `history`             | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | record_id             | -->
<!-- | change_time           | -->
<!-- | old_quantity          | -->
<!-- | new_quantity          | -->

<!-- | `karma`               | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | quantity              | -->
<!-- | expression            | -->

<!-- | `frequency`           | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | quantity              | -->
<!-- | day_week              | -->
<!-- | months                | -->
<!-- | days                  | -->
<!-- | seconds               | -->
<!-- | next_date             | -->
<!-- | finish_date           | -->
<!-- | when_done             | -->

<!-- | `sum`                 | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | quantity              | -->
<!-- | record_id             | -->
<!-- | sum_mode              | -->
<!-- | interval_mode         | -->
<!-- | interval_length       | -->
<!-- | end_lag               | -->
<!-- | end_date              | -->

<!-- | `command`             | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | quantity              | -->
<!-- | command               | -->

<!-- | `transfer`            | -->
<!-- |-----------------------| -->
<!-- | id                    | -->
<!-- | records_received      | -->
<!-- | records_contributed   | -->
<!-- | receiving_agreement   | -->
<!-- | contributing_agreement| -->
<!-- | agreement_time        | -->
<!-- | receiving_transfer_confirmation | -->
<!-- | contributing_transfer_confirmation | -->
<!-- | transfer_time         | -->
