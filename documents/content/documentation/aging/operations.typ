*Operations* \

There is a keyboard way of navigating through Lince, changing records and running commands. That is done through the Operations.
When one types anything in the frontend, an input will appear.

If you type '4c', a modal for creating a Record will pop up. Typing 'a' with a number, like 'a2' will make Configuration with id 2 be active,
and others inactive, an easy way to change colorscheme. One handy one is 's' with a number, 's1' to run a Shell command.

Just typing a number, like '23' will make the Record with id 23 have it's quantity become zero.

Writting 'k3' will change the active collection to be the one with Id 3, effectivelly changing screens.

Its possible to click in the collection to change it, and setting the quantity to zero by manually editing the field in the table.
But those operations are very frequent and can be more ergonomic for those that like to remember shortcuts.
Those that dont will have a hard time currently :(

#table(
  columns: 2,
  [[\#] Name], [[Key] Action],
  [[0] Configuration], [[c] Create],
  [[1] Collection], [[q] SQL Query],
  [[2] View], [[k] Change Collection],
  [[3] collection_View], [[s] Shell Command],
  [[4] Record], [[a] Activate Configuration],
  [[5] Karma_Condition], [],
  [[6] Karma_Consequence], [],
  [[7] Karma], [],
  [[8] Command], [],
  [[9] Frequency], [],
  [[10] Sum], [],
  [[11] History], [],
  [[12] DNA], [],
  [[13] Transfer], [],
)
