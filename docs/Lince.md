The Lince Way:

We model all aspects of the world with Records. It has 'head' and 'body' for title/description information and a 'quantity' for number. The central point of the Lince workflows is the changing of the 'quantity'.

Karma is an automation system that can take information such as the quantity of Records, calling (shell or SQL) Commands and Frequencies and evaluating (any Rust code that fits) it in a math equation called a Karma Condition. If it passes a threshold ('=') of being unequal to zero (or doesn't need to '=\*') it will carry the value onto the Karma Consequence. The Consequence can be the changing of the Record's quantity, the calling of (shell or SQL) Commands, Transfers being active, and more (syncing tree of Records between Organs).

The Frequency part of Karma is a built-in cron-job, example:

Condition (math): frequency-1 \* record-quantity-3
Threshold: =
Consequence: record-quantity-4

If this frequency is set to one month plus one day, when evaluated frequently (default Karma cycle happens every 60 seconds) it will return most of the time the number 0, only once per month plus one day it will return the number 1, so the math of almost always '0 \* something = 0) will make it not pass through the Threshold that stops zero amounts and not bring the Consequence. In this case the consequence is the changing of the quantity of the fourth Record (with id 4). So this Karma row is for the setting of record 4 to have the same quantity as Record 3 once every one month and one day.

The usage of Lince revolving around the Record's quantity means users can create rules that edits a collection of numbers as state, being more generic than fixed text options, Karma and Transfer following this creates pressure towards The Lince Way. It ss usefull to join several pieces of information with +-\*/() equations and creating business rules as much as possible with data (says Lince).

A Lince node is called an Organ, it can have many users inside or not. People can have users in various Organs, a personal one, family, work, project... And access them all through any one of them with login.

Two different Organs may want to use Records as the concept of Needs and Contributions to Needs. For executing interactions between users of Organs we have the Transfer feature, it can carry out donations, putting only the one way Contribution, or an economic trade by having a Need being met with an item/service Contribution one way and a Contribution for a money Need on the other way. That allows from buying items in digital market platforms to planning a party.
