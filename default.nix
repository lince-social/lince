{ pkgs ? import <nixpkgs> { } }:

let py = pkgs.python3Packages;

in pkgs.mkShell {
  buildInputs = [
    # Example 'py.datetime'
    pkgs.python3
    py.pandas
    py.datetime
    py.psycopg2
  ];
}

