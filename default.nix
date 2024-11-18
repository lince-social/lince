{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    git

    postgresql_16

    python312
    python312Packages.python-lsp-server
    python312Packages.pandas
    python312Packages.datetime
    python312Packages.psycopg2
    python312Packages.tabulate
    python312Packages.flask

    # nodePackages.nodejs
    nodejs_22

    graph-easy
    slides
  ];
  PGDATA = "${toString ./.}/.pg";
  shellHook = ''
    pg_ctl stop
    export PGHOST="$PGDATA"
    [ ! -d $PGDATA ] && pg_ctl initdb -o "-U postgres"
    pg_ctl -o "-p 5432 -k $PGDATA" start
    echo "log_min_messages = warning" >> $PGDATA/postgresql.conf
    echo "log_checkpoints = off" >> $PGDATA/postgresql.conf

    # [ $SCRIPT == 1 ] && slides ${toString ./documentation.md} || python ${toString ./src/flask_app.py}
    if [ -z "$SCRIPT" ]; then
        python ${toString ./src/flask_app.py}
    elif [ "$SCRIPT" -eq 1 ]; then
        python ${toString ./src/terminal.py}
    elif [ "$SCRIPT" -eq 2 ]; then
        slides ${toString ./documentation.md}
    else
        echo "Invalid SCRIPT value. Please use 1 for terminal.py, 2 for slides, or unset for flask_app.py."
    fi
  '';
}
