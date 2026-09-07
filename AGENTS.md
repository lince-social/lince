Instead of cargo build use cargo check. Warnings are treated as errors.

Do not spawn parallel agents for coding, do not make worktrees, code on the branch you are in. Make plans that can be followed sequentially and implemented step after step, your automatic context compressing should be enough to help you move from task to task.

Never touch AGENTS.md or README.md.
Never touch .lingua files unless explicitly asked, they are for user readability, simple language focused on what things are and how they work, plus eventual next tasks only the developers touch.
When planning things, you should just suggest a plan, never create a file that contains the plan, the bottleneck should always be the human manually typing or copypasting in .lingua files, that you should not touch.
Do not create markdowns, ever. In the past we thought making markdowns was a good idea. The current focus is in completing the work in the markdowns and slimming it until everything planned is either dropped or developed. Only suggest minimalist text, without complex words or technical jargon, that the user might want to put in the .lingua files.

The feature requests the human makes will be of frontend and/or backend. Make sure you code the respective frontend and backend part of features, so if there is extra UI and it's not reusing some backend feature you should build the frontend one. If it is purely backend then implement only that. If it envolves interaction of users in some form you should recommend the building of UI, the human will approve it with their comments. Whenever you finish a feature, with it's frontend/backend changes if needed, plus tests for correctness, performance and security if needed, you should read Lince.lingua and suggest the next task. That way we always are making a complete implementation of an idea, not leaving parts of it behind and suggesting to take more work. Do not think of Lince being in a phase or in a version, just code more things, let the human manage that.

Nobody uses Lince, never care about compatibility with older versions in any way.

The interface may use widgets/components called Sand, when you make a Sand that uses a dependency that is embeded you should also include it's LICENSE and credits (if exists) in the Sand.

