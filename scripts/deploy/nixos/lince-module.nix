# A NixOS service for a Lince Cell, in any of its four postures.
#
#   mode = "server"   `lince --server`: API only, no board, login forced on.
#   mode = "board"    `lince`: the full board over HTTP, for a browser.
#   mode = "desktop"  `lince-desktop`: the Tauri window, in your session.
#   mode = "front-door"  a Cell of YOUR Organ that carries and authors nothing.
#
# **This mode was called "relay" until 2026-08-15, and the rename is a
# correction, not a preference.** What it builds is your Organ's FRONT DOOR: a
# Cell inside your own signed roster, holding your plaintext store, unable to
# author anything. Lince uses "relay" for two other things — an `iroh-relay`,
# which is not Lince at all, and the blind mailbox that holds sealed envelopes
# for people who are not you (Ontology C4, unbuilt) — and an operator who read
# "relay" and deployed this got a box holding their data in the clear. That is
# precisely the blurring the design says must never happen.
#
# It is a MODE and not a flag on server, because almost nothing about it is a
# server with a switch flipped: it serves no board, needs no admin password
# (the assertion demanding one for `server` must not extend to it, and a
# matching one refuses a front door that is given one), and wants its own
# resource limits. What makes a Cell a front door is not this module at all —
# it is the Organ's signed roster giving that Cell no capabilities, which the
# database enforces. This module only shapes the unit around that fact.
#
# A server has no board, so the things the board configures are set from the
# shell instead, as the unit's own user:
#
#   sudo -u lince lince --data-dir /var/lib/lince discovery accept-unknown on
#   sudo -u lince lince --data-dir /var/lib/lince organ list
#   sudo -u lince lince --data-dir /var/lib/lince organ trust <uid> known
#   sudo -u lince lince --data-dir /var/lib/lince organ login <uid> <username>
#
# Those three decisions — open the pairing door, trust a contact, say which
# Person they act as — are what a peer needs before "Enter their Lince" works
# against this box. None of them is a NixOS option on purpose: the door should
# be shut again after pairing, and a declarative `acceptUnknown = true` would
# hold it open for the life of the machine.
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
        # A front door serves no board either, so it takes the same flag. The
        # difference between them is authority, not UI.
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
    }
    // lib.optionalAttrs (!isUserScope && cfg.mode == "front-door") {
      # A front door is a donation of somebody's bandwidth and RAM, and an
      # unbounded donation takes down the operator's other services before it
      # takes down Lince. Conservative rather than tuned: raise them
      # deliberately, having watched the box.
      MemoryMax = cfg.frontDoor.memoryMax;
      CPUQuota = cfg.frontDoor.cpuQuota;
      # It stores no conversations and hosts nobody's identity, so it needs far
      # less of the filesystem than a personal Cell.
      TasksMax = 512;
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
      (lib.optionalAttrs (!isDesktop) { XDG_CONFIG_HOME = "${cfg.dataDir}/.config"; }) // cfg.environment;
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
        "front-door"
      ];
      default = "server";
      description = ''
        "server" passes --server: no board UI, no sands, no static assets, and
        login is forced on. "board" serves the whole UI over HTTP — only bind
        that to loopback. "desktop" runs the Tauri app and requires
        scope = "user". "front-door" is a carrier for YOUR OWN Organ: same binary
        as server, no admin password, tighter limits. It holds no signing
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
          A front door has no board to log into and no authority to exercise — it
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
