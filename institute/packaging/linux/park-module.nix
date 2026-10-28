{ self }:
{ config, lib, pkgs, ... }:
let
  cfg = config.programs.lince-park;
  validDirectory = value: let
    parts = lib.filter (part: part != "") (lib.splitString "/" value);
  in
    lib.hasPrefix "/" value
    && lib.take 2 parts != [ "nix" "store" ]
    && !(lib.elem ".." parts);
  session = if !cfg.withUWSM && cfg.directory == null && cfg.sessionWrapper == [] then cfg.package else pkgs.runCommand "lince-park-session" {
    passthru.providedSessions = [ "lince" ];
  } ''
    mkdir -p "$out/bin" "$out/share/wayland-sessions"
    cat > "$out/bin/lince-park-session" <<'LAUNCHER'
    #!${pkgs.runtimeShell}
    ${lib.optionalString cfg.withUWSM "export LINCE_PARK_UWSM=${lib.escapeShellArg (lib.getExe config.programs.uwsm.package)}"}
    exec ${lib.escapeShellArgs (cfg.sessionWrapper ++ [ "${cfg.package}/bin/lince-park-session" ]
      ++ lib.optionals (cfg.directory != null) [ "--directory" cfg.directory ])} "$@"
    LAUNCHER
    chmod +x "$out/bin/lince-park-session"
    cp ${./wayland-sessions/lince.desktop} "$out/share/wayland-sessions/lince.desktop"
    substituteInPlace "$out/share/wayland-sessions/lince.desktop" \
      --replace-fail "Exec=lince-park-session" "Exec=$out/bin/lince-park-session"
  '';
in {
  options.programs.lince-park = {
    enable = lib.mkEnableOption "the Lince Wayland session";
    withUWSM = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Use the external UWSM session manager for Park startup and cleanup.";
    };
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.lince-park;
      description = "Lince executable with Sandbox and Park support.";
    };
    directory = lib.mkOption {
      type = lib.types.nullOr (lib.types.addCheck lib.types.str validDirectory);
      default = null;
      description = "Explicit single-user Cell directory; null keeps each login user's normal directory.";
    };
    sessionWrapper = lib.mkOption {
      type = lib.types.addCheck (lib.types.listOf lib.types.str)
        (arguments: arguments == [] || lib.hasPrefix "/" (builtins.head arguments));
      default = [];
      description = "Optional external session-manager executable and literal arguments prepended to the Park session command.";
    };
  };
  config = lib.mkIf cfg.enable {
    assertions = [{
      assertion = pkgs.stdenv.hostPlatform.isLinux;
      message = "Lince Park requires Linux.";
    }];
    environment.systemPackages = [ cfg.package ];
    services.displayManager.sessionPackages = lib.optionals (!cfg.withUWSM) [ session ];
    programs.uwsm = lib.mkIf cfg.withUWSM {
      enable = true;
      waylandCompositors.lince = {
        prettyName = "Lince (Park)";
        comment = "Lince Wayland desktop";
        binPath = "${session}/bin/lince-park-session";
      };
    };
    hardware.graphics.enable = true;
  };
}
