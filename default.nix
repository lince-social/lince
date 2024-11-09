{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    git

    postgresql_16

    python311
    python311Packages.python-lsp-server
    python311Packages.pandas
    python311Packages.datetime
    python311Packages.psycopg2
    python311Packages.tabulate
    python311Packages.urwid
    python311Packages.flask
    python311Packages.streamlit

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

    [ $SCRIPT == 2 ] && ( python ${toString ./src/flask_app.py}; python ${toString ./src/terminal.py}) || [ $SCRIPT == 1 ] && python ${toString ./src/flask_app.py} || python ${toString ./src/terminal.py} 
  '';
}
