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
  session = if cfg.directory == null && cfg.sessionWrapper == [] then cfg.package else pkgs.runCommand "lince-park-session" {
    passthru.providedSessions = [ "lince" ];
  } ''
    mkdir -p "$out/bin" "$out/share/wayland-sessions"
    cat > "$out/bin/lince-park-session" <<'LAUNCHER'
    #!${pkgs.runtimeShell}
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
    services.displayManager.sessionPackages = [ session ];
    hardware.graphics.enable = true;
  };
}
