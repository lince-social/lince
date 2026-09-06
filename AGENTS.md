Instead of cargo build use cargo check.
Warnings are treated as errors.
Do not use worktrees. You might be working alongside other agents, it is expected.
No comments in code sensei crate blocks running if they exist. To remove them do: SENSEI=off cargo run -p sensei --bin sensei -- fix && cargo fmt. There are license files with exceptions to this rule.

Never touch AGENTS.md or CLAUDE.md.
Never touch .lingua files unless explicitly asked, they are for user readability, simple language focused on what things are and how they work, plus eventual next tasks only the developers touch.
Never touch .md files unless given the permission. The .md in a root/anicca/directory/ are themed to a feature, free to edit only about such feature when working on it, or if requested. The maintained markdowns should be simplified to contain prose that can be put into .lingua files. In .lingua style, not complex blocks of jargon. Curated phrases for easy understanding.

Nobody uses Lince, never care about compatibility with older versions in any way.

The interface may use widgets/components called Sand, when you make a Sand that uses a dependency that is embeded you should also include it's LICENSE and credits (if exists) in the Sand.
Features that require UI to use should always build the UI part. If only the backend part is built it must be talked about so the HUMAN only edits the remaining tasks: building the frontend of it.

