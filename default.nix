{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    postgresql
    git
    python312
    python312Packages.python-lsp-server
    python312Packages.pandas
    python312Packages.datetime
    python312Packages.psycopg2
    python312Packages.tabulate

  ];
  PGDATA = "${toString ./.}/.pg";
  shellHook = ''
    pg_ctl stop
    export PGHOST="$PGDATA"

    [ ! -d $PGDATA ] && pg_ctl initdb -o "-U postgres"
    pg_ctl -o "-p 5432 -k $PGDATA" start
    python src/cli.py
  '';
}
