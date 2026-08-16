{
  description = "Lince";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      eachSystem = flake-utils.lib.eachSystem supportedSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          lib = pkgs.lib;
          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          version = cargoToml.workspace.package.version;
          cleanSrc = lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                name = baseNameOf path;
                rel = lib.removePrefix (toString ./. + "/") (toString path);
              in
              !(
                name == ".direnv"
                || name == ".devenv"
                || name == ".git"
                || name == "mprocs.log"
                || name == "target"
                || lib.hasPrefix "target/" rel
              );
          };
          tauriLinuxNativeBuildInputs =
            with pkgs;
            lib.optionals stdenv.isLinux [
              makeWrapper
              pkg-config
              wrapGAppsHook3
            ];
          tauriLinuxBuildInputs =
            with pkgs;
            lib.optionals stdenv.isLinux [
              gsettings-desktop-schemas
              glib-networking
              gst_all_1.gst-plugins-base
              gst_all_1.gst-plugins-good
              gst_all_1.gstreamer
              gtk3
              libayatana-appindicator
              libxkbcommon
              librsvg
              libsoup_3
              webkitgtk_4_1
              xdotool
            ];

          mkLince =
            { pname }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname version;
              src = cleanSrc;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              RUSTFLAGS = "-D warnings";

              cargoBuildFlags = [
                "--package"
                "lince"
              ];
              cargoTestFlags = [
                "--package"
                "lince"
              ];

              nativeBuildInputs = with pkgs; [
                pkg-config
              ];

              buildInputs = with pkgs; [
                openssl
                sqlite
              ];

              meta = {
                description = "Lince binary";
                mainProgram = "lince";
                license = lib.licenses.gpl3Plus;
                platforms = supportedSystems;
              };
            };

          lince = mkLince {
            pname = "lince";
          };

          lince-desktop = pkgs.rustPlatform.buildRustPackage {
            pname = "lince-desktop";
            inherit version;
            src = cleanSrc;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            RUSTFLAGS = "-D warnings";

            cargoBuildFlags = [
              "--package"
              "lince-desktop"
            ];
            cargoTestFlags = [
              "--package"
              "lince-desktop"
            ];

            nativeBuildInputs =
              with pkgs;
              [
                pkg-config
              ]
              ++ lib.remove pkg-config tauriLinuxNativeBuildInputs;

            buildInputs =
              (with pkgs; [
                openssl
                sqlite
              ])
              ++ tauriLinuxBuildInputs;

            postFixup = lib.optionalString pkgs.stdenv.isLinux ''
              wrapProgram "$out/bin/lince-desktop" \
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath tauriLinuxBuildInputs}"
            '';

            meta = {
              description = "Lince desktop webview application";
              mainProgram = "lince-desktop";
              license = lib.licenses.gpl3Plus;
              platforms = supportedSystems;
            };
          };
        in
        {
          packages = {
            default = lince;
            inherit lince lince-desktop;
          };

          apps = {
            default = flake-utils.lib.mkApp {
              drv = lince;
            };

            lince = flake-utils.lib.mkApp {
              drv = lince;
            };

            lince-desktop = flake-utils.lib.mkApp {
              drv = lince-desktop;
            };
          };

          formatter = pkgs.nixfmt-rfc-style;

          # Both relay jobs on one VPS, checked by EVALUATING the modules
          # rather than by reading them (Ontology C4). Linux only: a NixOS
          # configuration does not evaluate on darwin, and skipping is honest
          # where a stub would be a check that proves nothing.
          checks = lib.optionalAttrs (lib.hasSuffix "-linux" system) (
            let
              # Evaluate a host running BOTH jobs and hand back its config.
              # Only `assertions` and `systemd.services` are ever forced, so
              # this needs no bootloader, no filesystems and no real package —
              # it is an evaluation, not a machine.
              host =
                overrides:
                (nixpkgs.lib.nixosSystem {
                  inherit system;
                  modules = [
                    ./scripts/deploy/nixos/vps-module.nix
                    {
                      services.lince = {
                        enable = true;
                        mode = "front-door";
                        serverPackage = pkgs.hello;
                      };
                      services.iroh-relay = {
                        enable = true;
                        package = pkgs.hello;
                      };
                      # Silences a warning only; nothing here depends on it,
                      # because this evaluation never becomes a system.
                      system.stateVersion = "24.05";
                    }
                    overrides
                  ];
                }).config;

              # OUR assertions only. A configuration that is not a real machine
              # trips NixOS's own base ones — no root filesystem, no bootloader
              # — and those say nothing about whether the two jobs coexist.
              # Evaluating a whole bootable host to find that out would be a
              # much slower check answering a different question.
              ours =
                entry:
                lib.hasInfix "services.lince" entry.message || lib.hasInfix "services.iroh-relay" entry.message;
              failures = config: builtins.filter (entry: !entry.assertion && ours entry) config.assertions;
              # A false assertion IS a build failure — the module system
              # guarantees that — so asserting on the entry is exact and does
              # not depend on catching an evaluation error.
              refuses =
                name: overrides: fragment:
                let
                  found = failures (host overrides);
                in
                if found == [ ] then
                  throw "${name}: a blurred configuration evaluated cleanly; the assertion is not doing anything"
                else if !(lib.any (entry: lib.hasInfix fragment entry.message) found) then
                  throw "${name}: refused, but for the wrong reason: ${
                    lib.concatMapStringsSep " | " (entry: entry.message) found
                  }"
                else
                  true;

              # The other shape a refusal takes: an option conflict, which the
              # module system raises as an evaluation error rather than as an
              # assertion. Still a deployment that cannot happen, which is what
              # the box asks for — the message is nixpkgs' rather than ours.
              refusesToEvaluate =
                name: overrides:
                if (builtins.tryEval (builtins.deepSeq (host overrides).users.users true)).success then
                  throw "${name}: a blurred configuration evaluated cleanly"
                else
                  true;

              healthy = host { };
              lince-unit = healthy.systemd.services.lince.serviceConfig;
              relay-unit = healthy.systemd.services.iroh-relay.serviceConfig;
            in
            {
              vps-coexistence = pkgs.runCommand "vps-coexistence" { } (
                assert failures healthy == [ ];
                # Separate everything, asserted against the units that would
                # actually be installed rather than against the options.
                assert lince-unit.User != relay-unit.User;
                assert lince-unit.Group != relay-unit.Group;
                assert relay-unit.StateDirectory != (lince-unit.StateDirectory or null);
                # The structural half: a relay cannot see a home directory, and
                # a personal Cell's store lives in one.
                assert relay-unit.ProtectHome;
                assert relay-unit.ProtectSystem == "strict";
                # And each blurring the module exists to refuse.
                # Two different refusals for one mistake, and both are needed.
                # `lince-module` only declares `users.users.lince` when the
                # user is left at its default, so pointing the relay at
                # "lince" collides in the module system BEFORE any assertion
                # is read, while pointing both at a custom shared name reaches
                # ours. Testing only the first would have left the second
                # silently unguarded.
                assert refusesToEvaluate "shared default user" {
                  services.iroh-relay.user = "lince";
                };
                assert refuses "shared custom user" {
                  services.iroh-relay.user = "carrier";
                  services.lince.user = "carrier";
                } "user account";
                assert refuses "shared group" { services.iroh-relay.group = "lince"; } "shared group";
                assert refuses "port collision" {
                  services.lince.listenAddr = "0.0.0.0:80";
                } "will fail to start";
                assert refuses "store inside the relay's state" {
                  services.lince.dataDir = "/var/lib/iroh-relay/cell";
                } "plaintext at rest";
                "touch $out"
              );
            }
          );

          devShells.default = pkgs.mkShell {
            packages =
              (with pkgs; [
                cargo
                cargo-tauri
                clippy
                openssl
                pkg-config
                rust-analyzer
                rustc
                rustfmt
                sqlite
              ])
              ++ tauriLinuxNativeBuildInputs
              ++ tauriLinuxBuildInputs;

            shellHook = ''
              export RUSTFLAGS="-D warnings"
              export LINCE_MIGRATION_PREFLIGHT=1
            ''
            + lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${
                lib.makeLibraryPath (
                  tauriLinuxBuildInputs
                  ++ (with pkgs; [
                    openssl
                    sqlite
                  ])
                )
              }:''${LD_LIBRARY_PATH:-}"
              export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share:${pkgs.gtk3}/share:''${XDG_DATA_DIRS:-}"
              export GIO_EXTRA_MODULES="${pkgs.glib-networking}/lib/gio/modules:''${GIO_EXTRA_MODULES:-}"
              export GST_PLUGIN_SYSTEM_PATH_1_0="${
                lib.makeSearchPath "lib/gstreamer-1.0" (
                  with pkgs;
                  [
                    gst_all_1.gst-plugins-base
                    gst_all_1.gst-plugins-good
                    gst_all_1.gstreamer
                  ]
                )
              }:''${GST_PLUGIN_SYSTEM_PATH_1_0:-}"
              export GSETTINGS_SCHEMA_DIR="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}/glib-2.0/schemas"
            ''
            + ''
              if [[ -t 1 && -z "''${Lince_desktop_shell_started:-}" ]]; then
                export Lince_desktop_shell_started=1
                cd crates/desktop
                exec cargo tauri dev
              fi
            '';
          };

          devShells.desktop = pkgs.mkShell {
            packages =
              (with pkgs; [
                cargo
                cargo-tauri
                clippy
                openssl
                pkg-config
                rust-analyzer
                rustc
                rustfmt
                sqlite
              ])
              ++ tauriLinuxNativeBuildInputs
              ++ tauriLinuxBuildInputs;

            shellHook = ''
              export RUSTFLAGS="-D warnings"
              export LINCE_MIGRATION_PREFLIGHT=1
            ''
            + lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${
                lib.makeLibraryPath (
                  tauriLinuxBuildInputs
                  ++ (with pkgs; [
                    openssl
                    sqlite
                  ])
                )
              }:''${LD_LIBRARY_PATH:-}"
              export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share:${pkgs.gtk3}/share:''${XDG_DATA_DIRS:-}"
              export GIO_EXTRA_MODULES="${pkgs.glib-networking}/lib/gio/modules:''${GIO_EXTRA_MODULES:-}"
              export GST_PLUGIN_SYSTEM_PATH_1_0="${
                lib.makeSearchPath "lib/gstreamer-1.0" (
                  with pkgs;
                  [
                    gst_all_1.gst-plugins-base
                    gst_all_1.gst-plugins-good
                    gst_all_1.gstreamer
                  ]
                )
              }:''${GST_PLUGIN_SYSTEM_PATH_1_0:-}"
              export GSETTINGS_SCHEMA_DIR="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}/glib-2.0/schemas"
            ''
            + ''
              if [[ -t 1 && -z "''${Lince_desktop_shell_started:-}" ]]; then
                export Lince_desktop_shell_started=1
                cd crates/desktop
                exec cargo tauri dev
              fi
            '';
          };
        }
      );
    in
    eachSystem
    // {
      # One module for both postures. `services.lince.mode` picks between
      # `--server` (API only, login forced) and the full board; the desktop app
      # is a separate package (`lince-desktop`) rather than a mode of this one.
      nixosModules.default =
        { pkgs, ... }:
        {
          imports = [ ./scripts/deploy/nixos/lince-module.nix ];
          # Both, lazily: `lince-desktop` needs GTK/webkit and is only
          # evaluated if desktop mode actually asks for it.
          services.lince.serverPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince;
          services.lince.desktopPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince-desktop;
        };
      nixosModules.lince = self.nixosModules.default;

      # `iroh-relay` is a DEPENDENCY Lince can use, not part of it, which is
      # why it keeps its own `services.iroh-relay` namespace and its own system
      # user. Exported all the same: the module existed on disk and could not
      # be imported by flake reference, so the deployment the docs describe was
      # not actually reachable by anyone following them.
      # `package` is deliberately not defaulted here either — silently picking
      # a relay version is how you end up running one nobody chose.
      nixosModules.iroh-relay = ./scripts/deploy/nixos/iroh-relay-module.nix;

      # Both relay jobs on ONE VPS, kept apart. Imports the two modules above
      # and adds the assertions that make a blurred configuration fail to
      # evaluate rather than merely be unwise. See `vps-module.nix`.
      nixosModules.vps =
        { pkgs, ... }:
        {
          imports = [ ./scripts/deploy/nixos/vps-module.nix ];
          services.lince.serverPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince;
          services.lince.desktopPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince-desktop;
        };

      nixosConfigurations.manas-organ = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit self; };
        modules = [
          ./scripts/deploy/nixos/configuration.nix
        ];
      };
    };
}
