# A NixOS service for a Lince Cell, in either posture.
#
# `mode = "server"` runs `lince --server`: the API only, no board, login
# forced on. `mode = "board"` runs plain `lince`, which serves the full board
# to whoever can reach `listenAddr` — correct on a laptop bound to localhost,
# and a public board if you bind it to 0.0.0.0 without meaning to.
#
# The distinction is a runtime flag, not a build: one package, `lince`. The
# desktop app is a DIFFERENT package (`lince-desktop`, Tauri + GTK/webkit) and
# is not what this module runs.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.lince;
in
{
  options.services.lince = {
    enable = lib.mkEnableOption "the Lince Cell";

    package = lib.mkOption {
      type = lib.types.package;
      description = "The `lince` package (the headless binary, not lince-desktop).";
    };

    mode = lib.mkOption {
      type = lib.types.enum [
        "server"
        "board"
      ];
      default = "server";
      description = ''
        "server" passes --server: no board UI, no sands, no static assets, and
        login is forced on. "board" serves the whole UI — only bind that to a
        loopback address.
      '';
    };

    listenAddr = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:6174";
      description = "host:port to bind. Put a TLS reverse proxy in front of it.";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/lince";
      description = "Store, keys and lince.toml live here.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "lince";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "lince";
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
        assertion = cfg.mode != "board" || lib.hasPrefix "127.0.0.1:" cfg.listenAddr
          || lib.hasPrefix "localhost:" cfg.listenAddr || lib.hasPrefix "[::1]:" cfg.listenAddr;
        message = ''
          services.lince.mode = "board" serves the full board — anyone reaching
          ${cfg.listenAddr} gets a working board on this Cell's store and can put
          sands on it. Bind it to loopback, or use mode = "server".
        '';
      }
    ];

    users.users.${cfg.user} = lib.mkIf (cfg.user == "lince") {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      createHome = true;
    };
    users.groups.${cfg.group} = lib.mkIf (cfg.group == "lince") { };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [
      (lib.toInt (lib.last (lib.splitString ":" cfg.listenAddr)))
    ];

    systemd.services.lince = {
      description = "Lince Cell (${cfg.mode})";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
        ExecStart = lib.concatStringsSep " " (
          [
            "${cfg.package}/bin/lince"
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
        Restart = "always";
        RestartSec = 3;

        # The store is plaintext at rest by deliberate design (Ontology §11),
        # so the filesystem is the only thing protecting it.
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

      # `lince_data_dir()` falls back to $XDG_CONFIG_HOME/lince when no
      # --data-dir is given. --data-dir is passed above, so this only keeps
      # anything else the process writes inside the state directory.
      environment.XDG_CONFIG_HOME = "${cfg.dataDir}/.config";
    };
  };
}
