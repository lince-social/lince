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

# postgresql = {
#   authentication = pkgs.lib.mkOverride 10 ''
#       #type database  DBuser  auth-method
#       				local all       all     trust
#     enable = true;
#       				host all       all     127.0.0.1/32   trust
#       				host all       all     ::1/128        trust		
#       				'';
# };
