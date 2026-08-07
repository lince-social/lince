{
  config,
  pkgs,
  self,
  ...
}:

{
  imports = [ ./lince-module.nix ];

  system.stateVersion = "26.05";

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  networking.hostName = "manas-organ";
  networking.firewall.allowedTCPPorts = [
    22
    80
    443
  ];

  time.timeZone = "UTC";

  services.openssh.enable = true;

  environment.systemPackages = with pkgs; [
    git
    curl
    sqlite
  ];

  # Was `--http-api-only`, a flag that stopped existing in the May 2026
  # refactor. Nix does not validate unknown argv and the binary ignored it, so
  # this box had been serving the FULL BOARD to anyone reaching Caddy — the
  # opposite of what this line was here to do. `--server` (via mode) is the
  # working replacement.
  services.lince = {
    enable = true;
    package = self.packages.${pkgs.system}.lince;
    mode = "server";
    listenAddr = "127.0.0.1:6174";
    # Read on first boot only. Place it out of band (agenix/sops, or root-owned
    # mode 0600) — never a Nix string, which would be world-readable in the
    # store. Until it exists the unit fails fast rather than coming up as a
    # login wall with no accounts.
    initialAdminPasswordFile = "/var/lib/lince/initial-admin-password";
  };

  services.caddy = {
    enable = true;
    configFile = ../caddy/Caddyfile;
  };
}
