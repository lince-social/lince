{ inputs, ... }:
{
  imports = [ inputs.lince.nixosModules.park ];
  programs.lince-park.enable = true;
}
