# Scripts Migration Plan

## Goal

Organize repository automation under `scripts/` with a clearer split between:

- deployment
- installation
- maintainer utilities
- static service/config assets

## Principles

- keep entrypoints obvious
- separate deployment-specific commands from general installation flows
- structure larger shell scripts around small functions
- dispatch behavior by operating system where that changes installation behavior
- ask for explicit user confirmation before privileged or disruptive steps
- update references only after the target layout is stable

## Target Layout

Planned top-level structure:

- `scripts/deploy/`
- `scripts/install/`
- `scripts/other/`

### `scripts/deploy/`

Should contain only deployment-specific logic, such as:

- VPS deploy commands
- deploy polling / triggering helpers
- deploy-only Caddy assets
- deploy-only NixOS assets
- deploy-specific GitHub Actions helper files

### `scripts/install/`

Should contain user-facing installation logic, such as:

- binary installer
- user systemd installer
- system service installer
- multi-instance install helpers

### `scripts/other/`

Should contain maintainer and utility commands, such as:

- release helpers
- publish helpers
- backup helpers
- local developer utilities

## Installer Redesign

`install.sh` should be redesigned as a structured entrypoint rather than a long linear script.

### Desired Shape

1. parse arguments
2. detect operating system
3. collect consent for privileged actions
4. run shared validation helpers
5. dispatch to OS-specific functions
6. print clear next steps

### Desired Function Groups

- argument parsing
- environment validation
- download and checksum handling
- install destination setup
- privilege / consent prompts
- Linux-specific setup
- macOS-specific setup
- optional service installation
- final summary output

### Desired CLI Characteristics

The installer should eventually support a broader, organized set of arguments, for example:

- install destination controls
- release/version selection
- binary-only install
- service installation mode
- instance profile selection
- port selection
- data directory selection
- non-interactive mode

## Deployment Consolidation

Deployment should have a single clear command surface instead of several loosely related scripts.

### Desired Shape

- one main deploy entrypoint
- subcommands for trigger / worker / poll / install
- shared state/logging helpers
- explicit environment-variable contract for CI and VPS use

## Reference Migration

After the target layout is settled, references should be updated consistently across:

- GitHub Actions
- `flake.nix`
- `mise.toml`
- `README.md`
- website/bootstrap references
- helper scripts that call other scripts

## Risks

- CI breakage from stale paths
- deploy breakage from partially moved assets
- installer UX regressions if OS branching and privilege prompts are mixed carelessly
- documentation drift when script paths change faster than examples

## Suggested Order

1. stabilize deployment entrypoints
2. stabilize installation entrypoints
3. redesign `install.sh`
4. migrate remaining utility scripts
5. sweep and update all path references
6. remove obsolete layout remnants
