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
          interfaceLinuxNativeBuildInputs =
            with pkgs;
            lib.optionals stdenv.isLinux [
              makeWrapper
              pkg-config
            ];
          interfaceLinuxBuildInputs =
            with pkgs;
            lib.optionals stdenv.isLinux [
              libGL
              libdrm
              libgbm
              libx11
              libxcb
              libxcomposite
              libxdamage
              libxext
              libxfixes
              libxcursor
              libxi
              libxkbcommon
              libxrandr
              mesa
              vulkan-loader
              wayland
            ];

          mkLince =
            { pname, ui }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname version;
              src = cleanSrc;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              LINCE_REVISION = self.rev or self.dirtyRev or "unknown";

              RUSTFLAGS = "-D warnings";

              dontUseNinjaBuild = ui;
              dontUseNinjaCheck = ui;
              dontUseNinjaInstall = ui;

              cargoBuildFlags = [
                "--package"
                "lince"
              ]
              ++ lib.optional (!ui) "--no-default-features";
              cargoTestFlags = [
                "--package"
                "lince"
              ]
              ++ lib.optional (!ui) "--no-default-features";

              nativeBuildInputs =
                (with pkgs; [ pkg-config ])
                ++ lib.optionals ui (
                  [ pkgs.makeWrapper ] ++ lib.remove pkgs.pkg-config interfaceLinuxNativeBuildInputs
                );

              buildInputs =
                (with pkgs; [
                  openssl
                  sqlite
                ])
                ++ lib.optionals ui interfaceLinuxBuildInputs;

              postFixup = lib.optionalString (ui && pkgs.stdenv.isLinux) ''
                wrapProgram "$out/bin/lince" \
                  --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath interfaceLinuxBuildInputs}"
              '';

              meta = {
                description =
                  if ui then
                    "Lince: the Cell runtime and its native interface, one process"
                  else
                    "Lince, headless: the Cell runtime with no window";
                mainProgram = "lince";
                license = lib.licenses.gpl3Plus;
                platforms = supportedSystems;
              };
            };

          lince = mkLince {
            pname = "lince";
            ui = false;
          };

          lince-ui = mkLince {
            pname = "lince-ui";
            ui = true;
          };
        in
        {
          packages = {
            default = lince-ui;
            inherit lince lince-ui;
          };

          apps = {
            default = flake-utils.lib.mkApp {
              drv = lince-ui;
            };

            lince = flake-utils.lib.mkApp {
              drv = lince-ui;
            };

            lince-headless = flake-utils.lib.mkApp {
              drv = lince;
            };
          };

          formatter = pkgs.nixfmt;

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
                openssl
                pkg-config
                sqlite
              ])
              ++ interfaceLinuxNativeBuildInputs
              ++ interfaceLinuxBuildInputs;

            shellHook = ''
              export LINCE_MIGRATION_PREFLIGHT=1
            ''
            + lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${
                lib.makeLibraryPath (
                  interfaceLinuxBuildInputs
                  ++ (with pkgs; [
                    openssl
                    sqlite
                  ])
                )
              }:''${LD_LIBRARY_PATH:-}"
            '';
          };

          devShells.interface = pkgs.mkShell {
            packages =
              (
                with pkgs;
                [
                  openssl
                  pkg-config
                  python3
                  sqlite
                ]
                ++ lib.optionals stdenv.isLinux [
                  at-spi2-core
                  jq
                  orca
                ]
              )
              ++ interfaceLinuxNativeBuildInputs
              ++ interfaceLinuxBuildInputs;

            shellHook = ''
              export LINCE_MIGRATION_PREFLIGHT=1
            ''
            + lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${
                lib.makeLibraryPath (
                  interfaceLinuxBuildInputs
                  ++ (with pkgs; [
                    openssl
                    sqlite
                  ])
                )
              }:''${LD_LIBRARY_PATH:-}"
              export LINCE_AT_SPI_BUS_LAUNCHER="${pkgs.at-spi2-core}/libexec/at-spi-bus-launcher"
            '';
          };
        }
      );
    in
    eachSystem
    // {
      # One module for every posture. `services.lince.mode` picks between
      # `--server` (headless, login forced) and the windowed application, which
      # is the same binary built with its `ui` feature.
      nixosModules.default =
        { pkgs, ... }:
        {
          imports = [ ./scripts/deploy/nixos/lince-module.nix ];
          # Both, lazily: the windowed build is only evaluated if desktop mode
          # actually asks for it.
          services.lince.serverPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince;
          services.lince.uiPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince-ui;
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
          services.lince.uiPackage = nixpkgs.lib.mkDefault self.packages.${pkgs.system}.lince-ui;
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
