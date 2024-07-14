{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  # https://devenv.sh/packages/
  packages = with pkgs; [
    git
    python3
    python3Packages.pandas
    python3Packages.datetime
    python3Packages.psycopg2
  ];

  # https://devenv.sh/services/
  services.postgres.enable = true;

  enterShell = ''python `pwd`/src/app/lince.py'';

  # https://devenv.sh/languages/
  # languages.nix.enable = true;

  # https://devenv.sh/pre-commit-hooks/
  # pre-commit.hooks.shellcheck.enable = true;

  # https://devenv.sh/processes/
  # processes.ping.exec = "ping example.com";

  # See full reference at https://devenv.sh/reference/options/
}
