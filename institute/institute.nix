{
  pkgs,
  self,
  ...
}:

{
  imports = [ self.nixosModules.default ];

  system.stateVersion = "26.05";

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.initrd.availableKernelModules = [
    "virtio_pci"
    "virtio_blk"
    "virtio_scsi"
    "nvme"
    "xhci_pci"
    "ahci"
    "sd_mod"
  ];

  fileSystems."/" = {
    device = "/dev/disk/by-label/nixos";
    fsType = "ext4";
  };
  fileSystems."/boot" = {
    device = "/dev/disk/by-label/ESP";
    fsType = "vfat";
  };

  networking.hostName = "manas-organ";
  networking.useDHCP = true;
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

  services.lince = {
    enable = true;
    serverPackage = self.packages.${pkgs.stdenv.hostPlatform.system}.lince;
    mode = "server";
    listenAddr = "127.0.0.1:6174";
    initialAdminPasswordFile = "/var/lib/lince/initial-admin-password";
  };

  services.caddy = {
    enable = true;
    virtualHosts."institute.lince.social".extraConfig = ''
      encode gzip zstd
      reverse_proxy 127.0.0.1:6174
    '';
  };
}
