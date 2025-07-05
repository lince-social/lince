# Transfer

## Table: `transfer`

| transfer              | DATA STRUCTURE |
| --------------------- | -------------- |
| id                    | SERIAL         |
| records_received      | JSON           |
| records_contributed   | JSON           |
| agreement             | JSON           |
| agreement_time        | TIMESTAMP      |
| transfer_confirmation | JSON           |
| transfer_time         | TIMESTAMP      |

**WORK IN PROGRESS**
'records_received' is a collection of records and their quantities that will interact with our records, things you will receive. 'records_contributed' are the records you will contribute and their quantities, to the records of other parties, can be more than one party. So you can receive 5 moneys and an apple for driving someone from A to B. You don't work with it, but you have a car and their destination was on the way of yours. 'agreement' is a collection of agreement by all parties involved. 'agreement_time' is the moment every party agreed for the conditions of the trade, who will receive what. 'transfer_confirmation' is also a collection but with a confirmation from all parties that the transfer was successful, and 'transfer_time' for saving the event's moment.
