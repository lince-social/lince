{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell {
  buildInputs = with pkgs; [
    python3
    python3Packages.pandas
    python3Packages.datetime
    python3Packages.psycopg2
  ];
}
