# Command

| command  | DATA TYPE |
| -------- | --------- |
| id       | INTEGER   |
| quantity | INTEGER   |
| name     | TEXT      |
| command  | TEXT      |

The Command is a Shell command you can run in a bash Shell.

### Example:

| id  | quantity | command        |
| --- | -------- | -------------- |
| 1   |          | touch grass.el |

It is referenced in Karma Condition and/or Consequence as the letter 'c', followed by the id number, so this example would be 'c1'.
