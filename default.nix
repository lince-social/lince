{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [ devenv ];
  # python3
  # python3Packages.pandas
  # python3Packages.datetime
  # python3Packages.psycopg2
  shellHook = ''test -f `pwd`/devenv.nix && devenv shell || devenv init'';
}
