# Code, git and the merge

Lince holds the reasoning, git holds the code, Forgejo holds the merge. Each is good at one of those and bad at the other two. Putting all three in one server is a fine choice for a company that wants one server and the wrong one for a person who already has git working.

"Lince as a git remote" is not a feature, it is a prerequisite subsystem: a bundle of a live repository is tens to hundreds of megabytes, which neither the op log nor the thirty-day sealed mailbox is built to carry. Three levels exist and the cheapest is enough. A **reference** is a remote, a branch and a sha — a tiny string, expressible as ops today with zero new machinery. A **patch** is text, so it can be a Record body, which is what makes a review conversation self-contained: the thing being discussed travels with the discussion. A **bundle** is one file that would make Lince a real mirror, and it is the only rung that needs the blob store in `anicca/Files.md`.

Settled: reference now, patch at review time, bundle never by default. Nothing here waits on blob sync, which is what makes it buildable at all.

What the owner does today does not change — code on the laptop, commit locally, push to GitHub. A task becomes a Record and decomposes into `part-of` children. Work happens in a checkout on this machine and its result is written onto the task Record. The Record holds the reference, and under review the patch text too. On accept a bridge opens the pull request against the LAN or VPS Forgejo, or the same step targets GitHub; the PR body links back to the Record and the Record holds the PR url. Nothing about the repository moves through Lince. A contact holding the task Conversation sees the decisions, the patch and who authored them; cloning the code is `git clone`, because the code was never in Lince.

The crates: `gix` at `0.87.1` for reading a repository, `forgejo-api` at `0.11.1` for pull requests.

- [ ] Read branch and sha with `gix`, and hold a remote, a branch and a sha on the task Record.
- [ ] Put the patch text on the Record at review time, so the thing being discussed travels with the discussion.
- [ ] Open the pull request with `forgejo-api` on accept, link the Record from the PR body, and hold the PR url on the Record. GitHub stays the main remote.
- [ ] Check whether Forgejo's mirror should pull or push before committing to a direction.
