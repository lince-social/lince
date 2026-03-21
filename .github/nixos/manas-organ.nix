{ config, pkgs, self, ... }:

let
  domain = "manas.lince.social";
  lincePackage = self.packages.${pkgs.system}.lince-http;
in
{
  system.stateVersion = "25.05";

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  networking.hostName = "manas-organ";
  networking.firewall.allowedTCPPorts = [ 22 80 443 ];

  time.timeZone = "UTC";

  services.openssh.enable = true;

  users.users.lince = {
    isSystemUser = true;
    group = "lince";
    home = "/var/lib/lince";
    createHome = true;
  };

  users.groups.lince = {};

  environment.systemPackages = with pkgs; [
    git
    curl
    sqlite
  ];

  systemd.services.lince = {
    description = "Lince Social HTTP API";
    after = [ "network.target" ];
    wantedBy = [ "multi-user.target" ];

    serviceConfig = {
      Type = "simple";
      User = "lince";
      Group = "lince";
      WorkingDirectory = "/var/lib/lince";
      StateDirectory = "lince";
      EnvironmentFile = "/var/lib/lince/lince.env";
      ExecStart = "${lincePackage}/bin/lince";
      Restart = "always";
      RestartSec = 3;
    };

    environment = {
      HTTP_LISTEN_ADDR = "127.0.0.1:6174";
      XDG_CONFIG_HOME = "/var/lib/lince/.config";
    };
  };

  services.caddy = {
    enable = true;
    virtualHosts.${domain}.extraConfig = ''
      encode gzip zstd
      reverse_proxy 127.0.0.1:6174
    '';
  };
}
