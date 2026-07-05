| Command  | Data Type |
| -------- | --------- |
| Id       | Number    |
| Quantity | Number    |
| Name     | Text      |
| Command  | Text      |

The Command is a Shell command you can run in a bash Shell.

*Example*

| Id | Quantity | Command        |
| -- | -------- | -------------- |
| 1  |          | touch grass.html |

It is referenced in Karma Condition and/or Consequence as the letter 'c', followed by the id number, so this example would be 'c1'. The command above creates the file grass.html.