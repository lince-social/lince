# Karma startup activation

The owner requested booting directly into Park and waking a coordinator Fiote to
inspect unfinished tasks and delegate continued development. The running terminal
and agent process do not need to survive a reboot. The task's progress and next
action must be saved before restarting.

Create an ordinary Karma Rule with these three fields:

| Field | Value |
| --- | --- |
| Condition | `startup(@local-cell)` |
| Threshold | `!=0` |
| Consequence | `@coordinator: activate-fiote` |

Replace `coordinator` with the slug of an enabled, configured Fiote. Its pinned
instructions should inspect unfinished tasks, check interrupted effects and the
running Park revision, and delegate the Park task to the development Fiote.
Startup activation does not configure a provider, unlock credentials, resume an
old process or install a binary. Provider credentials must be available without
interactive unlocking for unattended operation.

The Cell emits startup after opening Fiote (including interruption recovery) and
initializing the local roster. Cells opened without background services, including
the current mobile and isolated runtimes, do not emit startup. Linux and Android
service hosts use the kernel boot ID; the
ordinary durable Rule application identity prevents the same Rule revision from
firing again when the Cell restarts during that boot. Other platforms use a new
identity per Cell process start. A new OS boot admits the Rule again. Changing a
Rule revision allows it to participate in the next Cell startup, including during
the same OS boot.

The reading requires one Cell Record and returns 1 only while evaluating that
Cell's startup event. It returns 0 during ordinary reactions, reading previews and
scheduled events. Saving a Rule does not emit startup. The startup hook selects
only active Rules directly reading the local Cell's startup; it does not scan all
tasks or infer a Fiote to activate. Conditions may combine the startup pulse with
other ordinary readings.

Startup uses existing Rule execution, permissions, executor selection, application
history and the durable effect queue. Paused Rules and stopped/unauthorized Karma
do not execute. A temporary database contention failure is retried by the Cell
host; other failures are reported through the existing Rule history and logs.
An interrupted run is still interrupted: the coordinator must inspect saved
work before requesting continuation.

The personal NixOS config in `/home/user/git/xaviduds/nixos` selects `lince-uwsm`,
enables SDDM autologin for `user`, and retains the greeter after logout. It keeps
the existing Park package wrapper and other installed sessions. Preparing a new
boot generation uses:

```sh
sudo nixos-rebuild boot --flake /home/user/git/xaviduds/nixos#computer
```

That command does not replace the running compositor. The startup feature must
be included in the installed Lince binary before the Rule can run.

## Validation

- The full personal NixOS system derivation evaluates, with `lince-uwsm` selected,
  autologin enabled and no failed assertions.
- `cargo xtask check --locked -p lince-cell` passes with warnings denied.
- Startup and Fiote activation tests: 5 passed. They cover concurrent duplicate
  startup calls, durable queued effects across engine reopen, another boot,
  unchanged Fiote quantity, actual history readings, pause/runtime/executor
  guards, ordinary Record changes, and invalid startup references.
- Existing Rule runtime and history tests: 16 passed.
- The broader Karma editor suite has 13 passes and two failures:
  `private_dependencies_cannot_be_revealed_through_a_derived_hover` and
  `rule_deletion_requires_permission_and_a_readable_target`. Both assert
  permission/visibility refusals in paths that do not use startup. These failures
  were not baseline-tested or repaired as part of startup activation.
- No applicable `|>` requirement was found in the Lince Records. Owner-authored
  `.lingua` files remain unchanged. No reboot or graphical boot acceptance was run.
