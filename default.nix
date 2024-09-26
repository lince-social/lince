{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    git

    postgresql

    python312
    python312Packages.python-lsp-server
    python312Packages.pandas
    python312Packages.datetime
    python312Packages.psycopg2
    python312Packages.tabulate
    python312Packages.urwid
    python312Packages.flask
    python312Packages.streamlit

    # nodePackages.nodejs
    nodejs_22
  ];
  PGDATA = "${toString ./.}/.pg";
  shellHook = ''
    pg_ctl stop
    export PGHOST="$PGDATA"
    [ ! -d $PGDATA ] && pg_ctl initdb -o "-U postgres"
    pg_ctl -o "-p 5432 -k $PGDATA" start
    echo "log_min_messages = warning" >> $PGDATA/postgresql.conf
    echo "log_checkpoints = off" >> $PGDATA/postgresql.conf

    # npm i @orbitdb/core helia @orbitdb/quickstart

    python ${toString ./src/terminal.py}
    # streamlit run ${toString ./src/streamlit.py}
  '';
}
