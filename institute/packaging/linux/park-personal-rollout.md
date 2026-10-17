# Park session proposal

This is an unapplied configuration proposal. No Nix evaluation, build, rebuild, login change or service activation has run during this source-only round.

The inspected configurations at `/home/user/git/xaviduds/nixos/configuration.nix` and `/home/user/git/xaviduds/lince/dotfiles/configuration.nix` enable SDDM, select Niri by default, and retain other desktop choices. Both flakes expose the `lince` input through `specialArgs`. The former tracks GitHub main; the latter uses the local Git repository. Neither input by itself proves the current working tree is the package that will be evaluated.

After automated and physical acceptance, import [park-session-proposal.nix](park-session-proposal.nix) into the actual system configuration being deployed. Its only choices are importing the existing Park module and enabling its session package. Keep the existing SDDM configuration, Niri and the default session. The proposal is portable to either inspected configuration; it has not been copied into either repository.

Before activation, identify the configuration actually used by the machine, evaluate its `computer` configuration against the intended Lince source and review the resulting system change. If testing the current uncommitted tree, explicitly use a path input so newly authored files are included. A Git input can omit untracked files. Record the evaluated source and executable identity with the test results. Do not update either personal flake lock or activate a system merely to prepare this proposal.

Use the packaged Lince entry for the final clean-login acceptance. Choose the external session supervisor, bar, launcher, locker, IME, clipboard-history helper and portal separately through local session configuration. Do not copy Niri-specific startup or portal IPC into Park. First confirm each selected provider supports the protocols Park implements. In particular, the installed Noctalia modules and portal version need live compatibility checks.

The ordinary Lince background-service settings found in the personal configurations are separate from this session entry. Review their directory and process ownership before the clean-login test so the same directory is not opened by competing Cells. Do not disable or rewrite that service as part of installing this proposal.

The user must authorize execution and any later system activation. The initial direct-TTY test can retain Niri and does not need this module to be installed.
