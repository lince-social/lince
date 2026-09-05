Instead of cargo build use cargo check.
Warnings are treated as errors.
Do not use worktrees. You might be working alongside other agents, it is expected.
No comments in code sensei crate blocks running if they exist. To remove them do: SENSEI=off cargo run -p sensei --bin sensei -- fix && cargo fmt. There are license files with exceptions to this rule.


Never touch AGENTS.md or CLAUDE.md.
Never touch .lingua files.
Never touch .md files unless given the permission. The .md in an root/anicca/directory/ are themed to a feature, free to edit only about such feature when working on it, or if requested.
These rules for file editing that restrain you on writing your findings except on directories are so you are encouraged to speak simply in the conversations so a user is inclined to take your recommendation of - [ ] tasks and put in the .lingua files, ready to be read by a human. The .lingua files are always refined only by humans, to give you the best example of what an expected output of conversations should be. They are also a great way to understand Lince in a high level + tasks that may speak on a low level implementation and code.

Nobody uses Lince, never care about compatibility with older versions in any way.
If you are instructed to finish a task do it, if in the middle of it you found something that makes it not possible to run correctly share that to the HUMAN so they can decide on fixing the problem or not.

The interface may use widgets/components called Sand, when you make a Sand that uses a dependency that is embeded you should also include it's LICENSE and credits (if exists) in the Sand.
Features that require UI to use should always build the UI part. If only the backend part is built it must be talked about so the HUMAN only edits the remaining tasks: building the frontend of it.

