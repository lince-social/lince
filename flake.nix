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
      binaryRelease =
        if builtins.pathExists ./binary.json then
          builtins.fromJSON (builtins.readFile ./binary.json)
        else
          null;

      linceModule =
        {
          config,
          lib,
          pkgs,
          ...
        }:

        let
          cfg = config.services.lince;

          isDesktop = cfg.mode == "desktop";
          isUserScope = cfg.scope == "user";

          package =
            if cfg.package != null then
              cfg.package
            else if isDesktop then
              cfg.uiPackage
            else
              cfg.serverPackage;

          execStart =
            if isDesktop then
              "${package}/bin/lince"
            else
              lib.concatStringsSep " " (
                [
                  "${package}/bin/lince"
                  "--directory"
                  cfg.dataDir
                  "--listen-addr"
                  cfg.listenAddr
                ]
                ++ lib.optional (cfg.mode == "server" || cfg.mode == "front-door") "--server"
                ++ lib.optionals (cfg.initialAdminPasswordFile != null) [
                  "--initial-admin-password-file"
                  (toString cfg.initialAdminPasswordFile)
                ]
                ++ lib.optionals (cfg.initialAdminPassword != null) [
                  "--initial-admin-password"
                  cfg.initialAdminPassword
                ]
              );

          unit = {
            description = "Lince (${cfg.mode})";
            wantedBy = [ (if isUserScope then "default.target" else "multi-user.target") ];
            after = [ "network.target" ];
            path = cfg.extraPath;

            serviceConfig = {
              Type = "simple";
              ExecStart = execStart;
              Restart = "always";
              RestartSec = 3;
            }
            // lib.optionalAttrs (!isDesktop) {
              WorkingDirectory = cfg.dataDir;
            }
            // lib.optionalAttrs (!isUserScope) {
              User = cfg.user;
              Group = cfg.group;
              ReadWritePaths = [ cfg.dataDir ];
              NoNewPrivileges = true;
              PrivateTmp = true;
              ProtectSystem = "strict";
              ProtectHome = true;
              ProtectKernelTunables = true;
              ProtectKernelModules = true;
              ProtectControlGroups = true;
              RestrictSUIDSGID = true;
            }
            // lib.optionalAttrs (!isUserScope && cfg.mode == "front-door") {
              MemoryMax = cfg.frontDoor.memoryMax;
              CPUQuota = cfg.frontDoor.cpuQuota;
              TasksMax = 512;
            };
          }
          // {
            environment =
              (lib.optionalAttrs (!isDesktop) { XDG_CONFIG_HOME = "${cfg.dataDir}/.config"; }) // cfg.environment;
          };
        in
        {
          options.services.lince = {
            enable = lib.mkEnableOption "the Lince Cell";

            mode = lib.mkOption {
              type = lib.types.enum [
                "server"
                "desktop"
                "front-door"
              ];
              default = "server";
              description = ''
                "server" passes --server: the Cell runs headless, with no window, and
                login is forced on. "desktop" runs the same crate built with its `ui`
                feature — one process holding the Cell and its native window — and
                requires scope = "user". "front-door" is a carrier for YOUR OWN Organ:
                same headless binary as server, no admin password, tighter limits. It holds no signing
                material because your Organ's roster gives it no capabilities — enrol
                it, then remove every capability from a device that holds your root key.
              '';
            };

            scope = lib.mkOption {
              type = lib.types.enum [
                "system"
                "user"
              ];
              default = "system";
              description = ''
                "system" is a daemon under its own account — what a server wants.
                "user" is a `systemd.user.services` unit inside your session, which is
                the only place a GUI app can live.
              '';
            };

            serverPackage = lib.mkOption {
              type = lib.types.package;
              description = "The headless `lince` package. Used by server and front-door modes.";
            };

            uiPackage = lib.mkOption {
              type = lib.types.package;
              description = "The windowed `lince` package (its `ui` feature). Used by desktop mode.";
            };

            package = lib.mkOption {
              type = lib.types.nullOr lib.types.package;
              default = null;
              description = "Override the package for the chosen mode. Rarely needed.";
            };

            frontDoor = {
              memoryMax = lib.mkOption {
                type = lib.types.str;
                default = "512M";
                description = ''
                  systemd MemoryMax for a front-door unit. It carries and forwards; it
                  holds no conversations, so this is generous rather than tight.
                '';
              };

              cpuQuota = lib.mkOption {
                type = lib.types.str;
                default = "50%";
                description = ''
                  systemd CPUQuota for a front-door unit. A ceiling so a busy one cannot
                  starve whatever else the operator runs on the box.
                '';
              };
            };

            listenAddr = lib.mkOption {
              type = lib.types.str;
              default = "127.0.0.1:6174";
              description = ''
                host:port this Cell names as its own local base URL. Ignored in desktop
                mode, which resolves its own.
              '';
            };

            dataDir = lib.mkOption {
              type = lib.types.path;
              default = "/var/lib/lince";
              description = ''
                Store, keys and lince.toml. Ignored in desktop mode, which resolves its
                own through XDG.
              '';
            };

            user = lib.mkOption {
              type = lib.types.str;
              default = "lince";
              description = "System scope only.";
            };

            group = lib.mkOption {
              type = lib.types.str;
              default = "lince";
              description = "System scope only.";
            };

            extraPath = lib.mkOption {
              type = lib.types.listOf lib.types.package;
              default = [ ];
              description = "Extra packages on the unit's PATH (a compiler for Karma, say).";
            };

            environment = lib.mkOption {
              type = lib.types.attrsOf lib.types.str;
              default = { };
              example = {
                HOME = "/home/user";
                XDG_DATA_HOME = "/home/user/.local/share";
              };
              description = ''
                Extra environment for the unit. Mainly for desktop mode, where the app
                finds its store through XDG and a thin session may not have set it.
                Wins over anything this module sets by default.
              '';
            };

            initialAdminPasswordFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = ''
                Path to a file holding the first admin's password, read on FIRST BOOT
                only (once an admin exists it is ignored). The file is read by the
                service, so it must be reachable BY THE UNIT: a system-scope unit runs
                as `user` with ProtectHome = true, so anything under /home is invisible
                to it regardless of permissions. Somewhere under `dataDir` works.

                Required in server mode unless `initialAdminPassword` is set: with no
                admin and no terminal to prompt on, `lince --server` refuses to start
                rather than come up as a login wall with no accounts.
              '';
            };

            initialAdminPassword = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = ''
                The first admin's password inline. Simpler than a file and fine for a
                box you alone administer, with the tradeoff stated plainly: a Nix
                string is world-readable in /nix/store, and it reaches the process as
                argv, so it is visible in `ps` and `systemctl cat lince` to any local
                user. Read on FIRST BOOT only, and ignored once an admin exists —
                so changing it later does nothing, and neither does removing it.

                Use `initialAdminPasswordFile` instead when anyone else can log into
                the machine.
              '';
            };

            openFirewall = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = "Open listenAddr's port. Leave off when a reverse proxy fronts it.";
            };
          };

          config = lib.mkIf cfg.enable {
            assertions = [
              {
                assertion =
                  cfg.mode != "server" || cfg.initialAdminPasswordFile != null || cfg.initialAdminPassword != null;
                message = ''
                  services.lince.mode = "server" forces login on, so the first boot needs
                  either services.lince.initialAdminPassword (inline; world-readable in
                  /nix/store and visible in `ps`) or services.lince.initialAdminPasswordFile
                  (out of band). Without one the unit starts, finds no admin and no TTY,
                  and exits — by design, so you get a failed unit instead of a running
                  login wall nobody can get into.
                '';
              }
              {
                assertion =
                  cfg.mode != "front-door"
                  || (cfg.initialAdminPasswordFile == null && cfg.initialAdminPassword == null);
                message = ''
                  services.lince.mode = "front-door" must NOT be given an admin password.
                  A front door has nothing to log into and no authority to exercise — it
                  carries traffic and authors nothing — so a password on it is either a
                  misunderstanding of what a front door is, or a login wall on a machine
                  that should not have one. Use mode = "server" if you meant a Cell
                  someone logs into.
                '';
              }
              {
                assertion = cfg.initialAdminPasswordFile == null || cfg.initialAdminPassword == null;
                message = ''
                  Set services.lince.initialAdminPassword OR
                  services.lince.initialAdminPasswordFile, not both. The binary prefers
                  the file and silently ignores the other, which is exactly the kind of
                  thing you would rather learn now than while wondering why a password
                  does not work.
                '';
              }
              {
                assertion = !isDesktop || isUserScope;
                message = ''
                  services.lince.mode = "desktop" runs a GUI application, which needs a
                  session — set services.lince.scope = "user". A system unit has no
                  display, and the hardening a system unit gets (ProtectHome) would stop
                  it reaching your own store anyway.
                '';
              }
            ];

            users.users.${cfg.user} = lib.mkIf (!isUserScope && cfg.user == "lince") {
              isSystemUser = true;
              group = cfg.group;
              home = cfg.dataDir;
              createHome = true;
            };
            users.groups.${cfg.group} = lib.mkIf (!isUserScope && cfg.group == "lince") { };

            networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [
              (lib.toInt (lib.last (lib.splitString ":" cfg.listenAddr)))
            ];

            systemd.services.lince = lib.mkIf (!isUserScope) unit;
            systemd.user.services.lince = lib.mkIf isUserScope unit;
          };
        };

      irohRelayModule =
        {
          config,
          lib,
          pkgs,
          ...
        }:

        let
          cfg = config.services.iroh-relay;

          configFile = pkgs.writeText "iroh-relay.toml" ''
            enable_relay = true
            http_bind_addr = "${cfg.httpBindAddr}"
            ${lib.optionalString (cfg.hostname != null) ''
              [tls]
              hostname = "${cfg.hostname}"
              cert_mode = "${cfg.certMode}"
              ${lib.optionalString (cfg.certsDir != null) ''cert_dir = "${cfg.certsDir}"''}
              https_bind_addr = "${cfg.httpsBindAddr}"
            ''}
            ${lib.optionalString cfg.enableStun ''
              [stun]
              enabled = true
              bind_addr = "${cfg.stunBindAddr}"
            ''}
            ${cfg.extraConfig}
          '';
        in
        {
          options.services.iroh-relay = {
            enable = lib.mkEnableOption "an iroh relay, for Lince and any other iroh application";

            package = lib.mkOption {
              type = lib.types.package;
              description = ''
                The package providing `bin/iroh-relay`. Not defaulted: this module does
                not vendor a relay, and picking one silently is how you end up running
                a version nobody chose.
              '';
            };

            hostname = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "relay.example.org";
              description = ''
                The public DNS name clients reach this relay at. A relay speaks
                HTTPS/WebSocket, so without a name and a certificate it can serve only
                plain HTTP — fine on a private network, wrong on the internet.
              '';
            };

            certMode = lib.mkOption {
              type = lib.types.enum [
                "letsencrypt"
                "manual"
              ];
              default = "letsencrypt";
              description = ''
                "letsencrypt" has the relay obtain its own certificate, which needs
                port 80 reachable. "manual" reads them from certsDir, which is what to
                use when something else already terminates or renews TLS.
              '';
            };

            certsDir = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "Where certificates live, for certMode = \"manual\".";
            };

            httpBindAddr = lib.mkOption {
              type = lib.types.str;
              default = "0.0.0.0:80";
              description = ''
                A relay is only useful if it is publicly reachable, so unlike a Lince
                board this binds wide by default — that is the job.
              '';
            };

            httpsBindAddr = lib.mkOption {
              type = lib.types.str;
              default = "0.0.0.0:443";
              description = "Where TLS is served, when a hostname is configured.";
            };

            enableStun = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = ''
                STUN is what lets two peers learn their own public addresses, which is
                what makes the upgrade to a direct path possible at all. Turning it off
                leaves every connection relayed for its whole life.
              '';
            };

            stunBindAddr = lib.mkOption {
              type = lib.types.str;
              default = "0.0.0.0:3478";
              description = "UDP address for STUN.";
            };

            openFirewall = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = ''
                Open the ports above. Default false, like everything else that exposes
                a service: opening a port should be a sentence someone wrote, not a
                side effect of enabling a module.
              '';
            };

            user = lib.mkOption {
              type = lib.types.str;
              default = "iroh-relay";
              description = ''
                Its OWN account, never the Lince one. A relay must not be able to read
                a Cell's store, and the cheapest way to guarantee that is for it to
                have no permission to.
              '';
            };

            group = lib.mkOption {
              type = lib.types.str;
              default = "iroh-relay";
              description = "The relay's own group.";
            };

            extraConfig = lib.mkOption {
              type = lib.types.lines;
              default = "";
              description = "Appended to the generated TOML verbatim.";
            };
          };

          config = lib.mkIf cfg.enable {
            assertions = [
              {
                assertion = cfg.certMode != "manual" || cfg.certsDir != null;
                message = ''
                  services.iroh-relay.certMode = "manual" needs
                  services.iroh-relay.certsDir. Without it the relay has nowhere to read
                  certificates from and will serve plain HTTP instead — which looks like
                  it is working right up until a client refuses to connect.
                '';
              }
              {
                assertion = cfg.hostname != null || cfg.httpsBindAddr == "0.0.0.0:443";
                message = ''
                  services.iroh-relay.httpsBindAddr was set without a hostname, so no
                  TLS will be configured and nothing will listen there. Set
                  services.iroh-relay.hostname.
                '';
              }
            ];

            users.users.${cfg.user} = {
              isSystemUser = true;
              group = cfg.group;
              description = "iroh relay";
            };
            users.groups.${cfg.group} = { };

            networking.firewall = lib.mkIf cfg.openFirewall {
              allowedTCPPorts = [ 80 ] ++ lib.optional (cfg.hostname != null) 443;
              allowedUDPPorts = lib.optional cfg.enableStun 3478;
            };

            systemd.services.iroh-relay = {
              description = "iroh relay";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                Type = "simple";
                ExecStart = "${cfg.package}/bin/iroh-relay --config-path ${configFile}";
                Restart = "always";
                RestartSec = 3;
                User = cfg.user;
                Group = cfg.group;
                StateDirectory = "iroh-relay";
                AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
                CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectSystem = "strict";
                ProtectHome = true;
                ProtectKernelTunables = true;
                ProtectKernelModules = true;
                ProtectControlGroups = true;
                RestrictSUIDSGID = true;
                MemoryMax = "1G";
                TasksMax = 4096;
              };
            };
          };
        };

      vpsModule =
        {
          config,
          lib,
          ...
        }:

        let
          lince = config.services.lince;
          relay = config.services.iroh-relay;
          both = lince.enable && relay.enable;

          portOf = address: lib.last (lib.splitString ":" address);
          lincePort = portOf lince.listenAddr;
          relayPorts = [
            (portOf relay.httpBindAddr)
          ]
          ++ lib.optional (relay.hostname != null) (portOf relay.httpsBindAddr);
        in
        {
          imports = [
            linceModule
            irohRelayModule
          ];

          config.assertions = lib.mkIf both [
            {
              assertion = relay.user != lince.user;
              message = ''
                services.iroh-relay.user and services.lince.user are both
                "${relay.user}". An iroh relay is not Lince — it is a dependency Lince
                can use — and sharing a user account undoes the separation the two
                modules are built around: the relay could then read the Cell's store,
                which is plaintext at rest by design.

                Leave both at their defaults ("iroh-relay" and "lince") unless you have
                a reason, and then give them different names.
              '';
            }
            {
              assertion = relay.group != lince.group;
              message = ''
                services.iroh-relay.group and services.lince.group are both
                "${relay.group}". A shared group is a shared read permission on the
                Cell's data directory, which is the thing the separate users exist to
                prevent.
              '';
            }
            {
              assertion = !(lib.elem lincePort relayPorts);
              message = ''
                services.lince.listenAddr (${lince.listenAddr}) and an
                services.iroh-relay bind address are both on port ${lincePort}. One of
                the two units will fail to start, and which one is a race.

                These are separate processes on one machine by design. Give the Cell
                its own port and reverse-proxy to it if it needs to be on 443.
              '';
            }
            {
              assertion = !(lib.hasPrefix "/var/lib/iroh-relay" lince.dataDir);
              message = ''
                services.lince.dataDir (${lince.dataDir}) is inside the iroh relay's
                state directory. The Cell's store is plaintext at rest, and the relay
                must not be able to reach it.

                The choice is readable-and-yours, or unreadable-and-anyone's. There is
                no middle option, and a shared directory is an attempt at one.
              '';
            }
          ];
        };

      supportedSystems =
        if binaryRelease != null then
          [ "x86_64-linux" ]
        else
          [
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
              rustPlatform.bindgenHook
            ];
          interfaceLinuxBuildInputs =
            with pkgs;
            lib.optionals stdenv.isLinux [
              alsa-lib
              pipewire
              glib
              dbus
              libpulseaudio
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
                outputHashes = {
                  "flexaudio-0.3.0" = "sha256-Cku6qBC22XC3xmtJ2u1oG3oUUEpX+XkP8UpSYk4qUWQ=";
                };
              };

              LINCE_REVISION = self.rev or self.dirtyRev or "unknown";

              RUSTFLAGS = "-D warnings";
              LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

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
                  [
                    pkgs.makeWrapper
                    pkgs.clang
                    pkgs.libclang
                  ]
                  ++ lib.remove pkgs.pkg-config interfaceLinuxNativeBuildInputs
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

          binaryPackage =
            let
              libraries = with pkgs; [
                stdenv.cc.cc.lib
                openssl
                sqlite
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
                alsa-lib
                libxkbcommon
                libxrandr
                vulkan-loader
                wayland
              ];
            in
            pkgs.stdenvNoCC.mkDerivation {
              pname = "lince";
              version = "${binaryRelease.version}-${builtins.substring 0 12 binaryRelease.revision}";
              src = pkgs.fetchurl {
                inherit (binaryRelease) url hash;
              };
              nativeBuildInputs = with pkgs; [
                autoPatchelfHook
                makeWrapper
              ];
              buildInputs = libraries;
              dontBuild = true;
              dontStrip = true;
              installPhase = ''
                runHook preInstall
                install -Dm755 bin/lince "$out/bin/lince"
                install -Dm644 LICENSE "$out/share/licenses/lince/LICENSE"
                install -Dm644 revision "$out/share/lince/revision"
                runHook postInstall
              '';
              postFixup = ''
                wrapProgram "$out/bin/lince" \
                  --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath libraries}"
              '';
              doInstallCheck = true;
              installCheckPhase = ''
                runHook preInstallCheck
                "$out/bin/lince" --help > /dev/null
                runHook postInstallCheck
              '';
              meta = {
                description = "Lince with its native interface and Cell runtime";
                mainProgram = "lince";
                license = lib.licenses.gpl3Plus;
                platforms = [ "x86_64-linux" ];
              };
            };

          lince =
            if binaryRelease != null && system == "x86_64-linux" then
              binaryPackage
            else
              mkLince {
                pname = "lince";
                ui = false;
              };

          lince-ui =
            if binaryRelease != null && system == "x86_64-linux" then
              binaryPackage
            else
              mkLince {
                pname = "lince-ui";
                ui = true;
              };
        in
        {
          packages = {
            default = lince-ui;
            inherit lince lince-ui;
          }
          // lib.optionalAttrs (system == "x86_64-linux") {
            goose = pkgs.stdenvNoCC.mkDerivation {
              pname = "goose-cli";
              version = "1.51.0";
              src = pkgs.fetchurl {
                url = "https://github.com/aaif-goose/goose/releases/download/v1.51.0/goose-x86_64-unknown-linux-musl.tar.gz";
                hash = "sha256-W/EbJCZHtP7yGC5Lvu65ocEpsyiDS56HTKTqCs7J2TU=";
              };
              sourceRoot = ".";
              dontStrip = true;
              installPhase = ''
                install -Dm755 goose "$out/bin/goose"
              '';
              meta = {
                description = "Goose ACP agent CLI";
                license = lib.licenses.asl20;
                mainProgram = "goose";
                platforms = [ "x86_64-linux" ];
              };
            };
          };

          apps = {
            default = (flake-utils.lib.mkApp {
              drv = lince-ui;
            }) // { meta.description = "Lince desktop"; };

            lince = (flake-utils.lib.mkApp {
              drv = lince-ui;
            }) // { meta.description = "Lince desktop"; };

            lince-headless = (flake-utils.lib.mkApp {
              drv = lince;
            }) // { meta.description = "Lince headless"; };
          };

          formatter = pkgs.nixfmt;

          # Both relay jobs on one VPS, checked by EVALUATING the modules
          # rather than by reading them (Ontology C4). Linux only: a NixOS
          # configuration does not evaluate on darwin, and skipping is honest
          # where a stub would be a check that proves nothing.
          checks =
            lib.optionalAttrs (lib.hasSuffix "-linux" system) (
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
                      vpsModule
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
            )
            // lib.optionalAttrs (binaryRelease != null) { binary = lince; };

          devShells.default = pkgs.mkShell {
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            packages =
              (with pkgs; [
                openssl
                pkg-config
                sqlite
                clang
                libclang
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

          devShells.media = pkgs.mkShell {
            inputsFrom = [ self.devShells.${system}.interface ];
            packages =
              with pkgs;
              [
                clang
                libclang
              ]
              ++ lib.optionals stdenv.isLinux [
                pipewire
                glib
                dbus
                libpulseaudio
              ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          };

          devShells.interface = pkgs.mkShell {
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            packages =
              (
                with pkgs;
                [
                  openssl
                  pkg-config
                  python3
                  sqlite
                  clang
                  libclang
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
          imports = [ linceModule ];
          # Both, lazily: the windowed build is only evaluated if desktop mode
          # actually asks for it.
          services.lince.serverPackage =
            nixpkgs.lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.lince;
          services.lince.uiPackage =
            nixpkgs.lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.lince-ui;
        };
      nixosModules.lince = self.nixosModules.default;

      # `iroh-relay` is a DEPENDENCY Lince can use, not part of it, which is
      # why it keeps its own `services.iroh-relay` namespace and its own system
      # user. Exported all the same: the module existed on disk and could not
      # be imported by flake reference, so the deployment the docs describe was
      # not actually reachable by anyone following them.
      # `package` is deliberately not defaulted here either — silently picking
      # a relay version is how you end up running one nobody chose.
      nixosModules.iroh-relay = irohRelayModule;

      # Both relay jobs on ONE VPS, kept apart. Imports the two modules above
      # and adds the assertions that make a blurred configuration fail to
      # evaluate rather than merely be unwise. See `vps-module.nix`.
      nixosModules.vps =
        { pkgs, ... }:
        {
          imports = [ vpsModule ];
          services.lince.serverPackage =
            nixpkgs.lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.lince;
          services.lince.uiPackage =
            nixpkgs.lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.lince-ui;
        };

      nixosConfigurations.manas-organ = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit self; };
        modules = [
          ./institute/institute.nix
        ];
      };
    };
}
