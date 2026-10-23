{ self, nixpkgs, pkgs, system }:
let
  inherit (pkgs) lib;
  fixture = pkgs.runCommand "park-package-fixture" { passthru.providedSessions = [ "lince" ]; } ''
    mkdir -p "$out/bin" "$out/share/wayland-sessions"
    cp ${./wayland-sessions/lince.desktop} "$out/share/wayland-sessions/lince.desktop"
    cat > "$out/bin/lince-park-session" <<'LAUNCHER'
    #!${pkgs.runtimeShell}
    printf '%s\n' "$@"
    LAUNCHER
    chmod +x "$out/bin/lince-park-session"
  '';
  host = overrides: (nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [ self.nixosModules.park {
      programs.lince-park.package = fixture;
      services.displayManager.sddm.enable = true;
      services.displayManager.defaultSession = "niri";
      programs.niri.enable = true;
      system.stateVersion = "26.05";
    } overrides ];
  }).config;
  enabled = host { programs.lince-park.enable = true; };
  disabled = host { };
  bare = (nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [ self.nixosModules.park {
      programs.lince-park = { enable = true; package = fixture; };
      system.stateVersion = "26.05";
    } ];
  }).config;
  directory = "/home/preview/a'b $value";
  manager = pkgs.writeShellScript "park-manager-fixture" ''
    test "$1" = 'literal $value'
    shift
    exec "$@"
  '';
  custom = host { programs.lince-park = { enable = true; inherit directory; sessionWrapper = [ (toString manager) "literal $value" ]; }; };
  selected = builtins.head custom.services.displayManager.sessionPackages;
  refuses = directory: !(builtins.tryEval (builtins.deepSeq (host {
    programs.lince-park = { enable = true; inherit directory; };
  }).programs.lince-park.directory true)).success;
in pkgs.runCommand "park-session-contracts" { } (
  assert lib.elem fixture enabled.services.displayManager.sessionPackages;
  assert !(lib.elem fixture disabled.services.displayManager.sessionPackages);
  assert enabled.hardware.graphics.enable;
  assert enabled.services.displayManager.sddm.enable;
  assert enabled.services.displayManager.defaultSession == "niri";
  assert enabled.programs.niri.enable;
  assert !bare.services.displayManager.sddm.enable;
  assert !bare.services.displayManager.gdm.enable;
  assert !bare.services.greetd.enable;
  assert !(bare.systemd.user.services ? lince);
  assert !(enabled.systemd.user.services ? lince);
  assert enabled.programs.lince-park.directory == null;
  assert enabled.programs.lince-park.sessionWrapper == [];
  assert selected.providedSessions == [ "lince" ];
  assert builtins.length custom.services.displayManager.sessionPackages == builtins.length enabled.services.displayManager.sessionPackages;
  assert refuses "relative/cell";
  assert refuses "/nix/store/cell";
  assert refuses "/nix//store/cell";
  assert refuses "/tmp/../nix/store/cell";
  ''
    ${selected}/bin/lince-park-session 'literal $(touch unexpected)' > arguments
    printf '%s\n' --directory ${lib.escapeShellArg directory} 'literal $(touch unexpected)' > expected
    cmp arguments expected
    test ! -e unexpected
    grep -Fx "Exec=${selected}/bin/lince-park-session" ${selected}/share/wayland-sessions/lince.desktop
    mkdir -p "$out"
    cp arguments "$out/arguments"
  ''
)
