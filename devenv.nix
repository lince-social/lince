{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
{
  packages = with pkgs; [
    git
    python3
    python3Packages.pandas
    python3Packages.datetime
    python3Packages.psycopg2
  ];

  services.postgres.enable = true;

  enterShell = ''python `pwd`/src/app/lince.py'';
}
