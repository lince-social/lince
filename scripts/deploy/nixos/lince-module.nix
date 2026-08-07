# A NixOS service for a Lince Cell, in any of its three postures.
#
#   mode = "server"   `lince --server`: API only, no board, login forced on.
#   mode = "board"    `lince`: the full board over HTTP, for a browser.
#   mode = "desktop"  `lince-desktop`: the Tauri window, in your session.
#
# Server and board are the SAME binary and differ by one runtime flag. Desktop
# is a genuinely different package (Tauri + GTK/webkit) and belongs to a user
# session, not to the system — so it requires `scope = "user"`, which is what
# the assertions below enforce rather than leave you to discover.
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
      cfg.desktopPackage
    else
      cfg.serverPackage;

  # `lince-desktop` takes no --data-dir or --listen-addr: it binds its own
  # loopback port and finds its store through XDG. Passing them would be args
  # it silently ignores — the failure mode this module exists to avoid.
  execStart =
    if isDesktop then
      "${package}/bin/lince-desktop --desktop-autostart"
    else
      lib.concatStringsSep " " (
        [
          "${package}/bin/lince"
          "--data-dir"
          cfg.dataDir
          "--listen-addr"
          cfg.listenAddr
        ]
        ++ lib.optional (cfg.mode == "server") "--server"
        ++ lib.optionals (cfg.initialAdminPasswordFile != null) [
          "--initial-admin-password-file"
          (toString cfg.initialAdminPasswordFile)
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
      # The store is plaintext at rest by deliberate design (Ontology §11), so
      # the filesystem is the only thing protecting it. None of this is applied
      # to a user-scope desktop app, where ProtectHome alone would break it.
      ReadWritePaths = [ cfg.dataDir ];
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ProtectKernelTunables = true;
      ProtectKernelModules = true;
      ProtectControlGroups = true;
      RestrictSUIDSGID = true;
    };
  }
  // {
    # `lince_data_dir()` falls back to $XDG_CONFIG_HOME/lince when no
    # --data-dir is given; --data-dir is passed above, so this only keeps
    # anything else the process writes inside the state directory. The desktop
    # app gets no default here deliberately: it finds its store through the
    # session's own XDG, which is the whole reason it can share one with your
    # shell. `cfg.environment` is last, so it wins either way.
    environment =
      (lib.optionalAttrs (!isDesktop) { XDG_CONFIG_HOME = "${cfg.dataDir}/.config"; })
      // cfg.environment;
  };
in
{
  options.services.lince = {
    enable = lib.mkEnableOption "the Lince Cell";

    mode = lib.mkOption {
      type = lib.types.enum [
        "server"
        "board"
        "desktop"
      ];
      default = "server";
      description = ''
        "server" passes --server: no board UI, no sands, no static assets, and
        login is forced on. "board" serves the whole UI over HTTP — only bind
        that to loopback. "desktop" runs the Tauri app and requires
        scope = "user".
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
      description = "The headless `lince` package. Used by server and board modes.";
    };

    desktopPackage = lib.mkOption {
      type = lib.types.package;
      description = "The `lince-desktop` (Tauri) package. Used by desktop mode.";
    };

    package = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = "Override the package for the chosen mode. Rarely needed.";
    };

    listenAddr = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:6174";
      description = ''
        host:port to bind. Put a TLS reverse proxy in front of it. Ignored in
        desktop mode, which binds its own loopback port.
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
        only (once an admin exists it is ignored). Use a secret manager
        (agenix/sops) or a root-owned mode-0600 file — never a Nix string
        literal, which would land world-readable in the store.

        Required in server mode: with no admin and no terminal to prompt on,
        `lince --server` refuses to start rather than come up as a login wall
        with no accounts.
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
        assertion = cfg.mode != "server" || cfg.initialAdminPasswordFile != null;
        message = ''
          services.lince.mode = "server" forces login on, so the first boot needs
          services.lince.initialAdminPasswordFile. Without it the unit starts,
          finds no admin and no TTY, and exits — by design, so you get a failed
          unit instead of a running login wall nobody can get into.
        '';
      }
      {
        assertion =
          cfg.mode != "board"
          || lib.hasPrefix "127.0.0.1:" cfg.listenAddr
          || lib.hasPrefix "localhost:" cfg.listenAddr
          || lib.hasPrefix "[::1]:" cfg.listenAddr;
        message = ''
          services.lince.mode = "board" serves the full board — anyone reaching
          ${cfg.listenAddr} gets a working board on this Cell's store and can put
          sands on it. Bind it to loopback, or use mode = "server".
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
}
