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
              cmake
              makeWrapper
              ninja
              pkg-config
            ];
          interfaceLinuxBuildInputs =
            with pkgs;
            lib.optionals stdenv.isLinux [
              alsa-lib
              atk
              cairo
              cups
              dbus
              expat
              fontconfig
              freetype
              glib
              gtk3
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
              nspr
              nss
              pango
              systemdLibs
              vulkan-loader
              wayland
            ];
          cefLinuxArchive =
            if system == "x86_64-linux" then
              {
                name = "cef_binary_151.3.24+g2384915+chromium-151.0.7922.174_linux64_minimal.tar.bz2";
                hash = "sha256-21PEP9rOi37krw8AUSARbWlzqp2Ot3AsBhe635voaE4=";
                sha1 = "b1e99d3e3ff4213f99f7cda0211db89454398811";
              }
            else if system == "aarch64-linux" then
              {
                name = "cef_binary_151.3.24+g2384915+chromium-151.0.7922.174_linuxarm64_minimal.tar.bz2";
                hash = "sha256-R5ZbnDallYvdbW/bP+M2DzjRfWWRTvY2q63hSIHNxZs=";
                sha1 = "95acd2a46975e2c60afa6b427ec50c0a3be6236f";
              }
            else
              null;
          cefLinuxRuntime =
            if cefLinuxArchive == null then
              null
            else
              pkgs.stdenvNoCC.mkDerivation {
                pname = "lince-cef-runtime";
                version = "151.3.24";
                src = pkgs.fetchurl {
                  url = "https://cef-builds.spotifycdn.com/${cefLinuxArchive.name}";
                  inherit (cefLinuxArchive) hash;
                };
                nativeBuildInputs = with pkgs; [
                  autoPatchelfHook
                  bzip2
                ];
                buildInputs = interfaceLinuxBuildInputs;
                sourceRoot = ".";
                unpackPhase = ''
                  tar -xjf "$src" --strip-components=1
                '';
                installPhase = ''
                  mkdir -p "$out"
                  cp CMakeLists.txt CREDITS.html LICENSE.txt "$out/"
                  cp -R cmake include libcef_dll "$out/"
                  cp -R Release/. "$out/"
                  cp -R Resources/. "$out/"
                  printf '%s\n' '${
                    builtins.toJSON {
                      type = "minimal";
                      inherit (cefLinuxArchive) name sha1;
                    }
                  }' > "$out/archive.json"
                '';
              };
          interfaceCefShellHook = lib.optionalString pkgs.stdenv.isLinux ''
            cef_work_path="$PWD/target/interface-cef/${cefLinuxRuntime.name}"
            if [[ ! -e "$cef_work_path/.ready" ]]; then
              mkdir -p "$cef_work_path"
              chmod -R u+w "$cef_work_path"
              cp -R --reflink=auto ${cefLinuxRuntime}/. "$cef_work_path/"
              chmod -R u+w "$cef_work_path"
              touch "$cef_work_path/.ready"
            fi
            export CEF_PATH="$cef_work_path"
          '';

          mkLince =
            { pname }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname version;
              src = cleanSrc;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              LINCE_REVISION = self.rev or self.dirtyRev or "unknown";

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

            dontUseNinjaBuild = true;
            dontUseNinjaCheck = true;
            dontUseNinjaInstall = true;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            LINCE_REVISION = self.rev or self.dirtyRev or "unknown";

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
              ++ lib.remove pkg-config interfaceLinuxNativeBuildInputs;

            buildInputs =
              (with pkgs; [
                openssl
                sqlite
              ])
              ++ interfaceLinuxBuildInputs;

            postFixup = lib.optionalString pkgs.stdenv.isLinux ''
              wrapProgram "$out/bin/lince-desktop" \
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath interfaceLinuxBuildInputs}"
            '';

            meta = {
              description = "Lince native desktop application";
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
                cmake
                ninja
                openssl
                pkg-config
                sqlite
              ])
              ++ lib.optionals (!pkgs.stdenv.isLinux) [ pkgs.cargo-tauri ]
              ++ interfaceLinuxNativeBuildInputs
              ++ interfaceLinuxBuildInputs;

            shellHook = ''
              export LINCE_MIGRATION_PREFLIGHT=1
            ''
            + interfaceCefShellHook
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
                  cmake
                  ninja
                  pkg-config
                  python3
                ]
                ++ lib.optionals stdenv.isLinux [
                  at-spi2-core
                  jq
                  orca
                ]
              )
              ++ interfaceLinuxBuildInputs;

            shellHook =
              interfaceCefShellHook
            + lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${lib.makeLibraryPath interfaceLinuxBuildInputs}:''${LD_LIBRARY_PATH:-}"
              export LINCE_AT_SPI_BUS_LAUNCHER="${pkgs.at-spi2-core}/libexec/at-spi-bus-launcher"
            '';
          };

          devShells.legacy = pkgs.mkShell {
            packages = with pkgs; [
              cmake
              curl
              ninja
              openssl
              pkg-config
              sqlite
              xdg-utils
            ];

            shellHook = ''
              export LINCE_MIGRATION_PREFLIGHT=1
            '';
          };

          devShells.desktop = pkgs.mkShell {
            packages =
              (with pkgs; [
                cmake
                ninja
                openssl
                pkg-config
                sqlite
              ])
              ++ lib.optionals (!pkgs.stdenv.isLinux) [ pkgs.cargo-tauri ]
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
          # Both, lazily: `lince-desktop` is only
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
