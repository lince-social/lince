### Command buffer:

      - [ ] The command buffer doesnt always appear. At least when running the command with Operation Input.
      - [ ] Maintain the shell's text highlighting. Maybe using tree-sitter?

# Multiple Languages

syntax highlighting and lsp (Tree-Sitter?) for Commands Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash, if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.

syntax highlighting and lsp (Tree-Sitter?) for Commands
Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash,
if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.

| Command  | Data Type |
| -------- | --------- |
| Id       | Number    |
| Quantity | Number    |
| Name     | Text      |
| Command  | Text      |

The Command is a Shell command you can run in a bash Shell.

_Example_

| Id  | Quantity | Command          |
| --- | -------- | ---------------- |
| 1   |          | touch grass.html |

It is referenced in Karma Condition and/or Consequence as the letter 'c', followed by the id number, so this example would be 'c1'. The command above creates the file grass.html.
