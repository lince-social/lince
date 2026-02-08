This is the document with guidelines on how to contribute to Lince. It contains information on how to name commits and how to navigate the commands to run the project. Create an issue or discussion if you have any questions or suggestions.

# Versioning

To automatically generate new versions and releases, it is essential to follow a commit standard: [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

This project uses the [Git Cliff](https://github.com/orhun/git-cliff) crate together with the `mask release` command to **automatically bump the version**, creating tags and releases on GitHub and updating the `CHANGELOG.md`.

# Recommended Commit Structure

\<type\>(\<optional scope\>): \<short message\>

- **type**: `fix`, `feat`, `perf`, `security`, etc.
- **scope**: affected module or component (optional, but recommended).
- **message**: short description of what changed.

> Examples:
>
> ```bash
> git commit -m "fix(auth): fix login bug"
> git commit -m "feat(export): add PDF export"
> git commit -m "perf(db): optimize report query"
> git commit -m "security(auth): fix CSRF vulnerability"
> git commit -m "feat(api): new login route (BREAKING CHANGE)"
> ```

# Commit Types and Version Impact

| Commit Prefix     | Version Bump    | Meaning                                                  |
| ----------------- | --------------- | -------------------------------------------------------- |
| `fix`             | Patch (`*.*.X`) | Bug fix without changing existing functionality.         |
| `perf`            | Patch (`*.*.X`) | Performance improvement without breaking compatibility.  |
| `security`        | Patch (`*.*.X`) | Security vulnerability fix.                              |
| `feat`            | Minor (`*.X.*`) | New feature or compatible improvement.                   |
| `BREAKING CHANGE` | Major (`X.*.*`) | Change that breaks compatibility with previous versions. |

> ⚠️ Commits with prefixes such as `chore`, `build`, `ci`, `docs`, `style`, `test`, or `refactor` **do not trigger a version bump**, but still appear in the changelog.

# Commands

The application can be run using [mask](https://github.com/jacobdeichert/mask), installed with:

```bash
cargo install mask
```

The main command is `cargo run -- karma gui`, which runs Karma and the GPUI frontend.

Check the `mise.md` for more information about the workflow.
