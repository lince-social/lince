Dont add comments.
Instead of cargo build use cargo check.

If the prompt mentions a task inside tasks.typ, you should try to complete the items inside it:
Do the - [/] items first, if there are multiple do them all marked as - [/], if there are none, do the - [ ] ones. The user may prompt you to continue because there are new items, or you didnt complete the ones they asked previously. Dont waste tokens speaking about doing the items that are - [/] or - [ ], just change the task from - [/] or - [ ] to - [x] if you completed it. Only try to do the Task the user asked, take up items from other tasks
when explicitly asked, if there are no more tasks to complete ask what other tasks you must do.

If you need more information about certain topics look at the documentation in documents/content/documentation/aging. It contains information about current implementations but also ideas from other past implementations of Lince, so take it with a grain of salt. If you dont think you understand something crucial, ask the user. The current code is supposed to be a more up to date reference to the functioning of the app, so consider it higher than the documentation for validation of business rules.
