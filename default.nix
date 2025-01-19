{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  buildInputs = with pkgs; [
    postgresql_17
    nodejs_23
    prisma-engines
    prisma
    pnpm
  ];

  PRISMA_QUERY_ENGINE_BINARY = "${pkgs.prisma-engines}/bin/query-engine";
  PRISMA_QUERY_ENGINE_LIBRARY = "${pkgs.prisma-engines}/lib/libquery_engine.node";
  PRISMA_INTROSPECTION_ENGINE_BINARY = "${pkgs.prisma-engines}/bin/introspection-engine";
  PRISMA_FMT_BINARY = "${pkgs.prisma-engines}/bin/prisma-fmt";
  # PRISMA_ENGINES_CHECKSUM_IGNORE_MISSING = "1";

  PGDATA = "${toString ./.}/.pg";

  shellHook = ''
    # Ensure PostgreSQL is initialized and started
    # if [ ! -f "$PGDATA/postmaster.pid" ]; then
    #   echo "Initializing PostgreSQL on port 2000..."
    #   pg_ctl initdb -D "$PGDATA" -U postgres
    #   echo "port = 2000" >> $PGDATA/postgresql.conf  # Ensure port 2000 is configured
    #   pg_ctl start -D "$PGDATA" -o "-p 2000"
    # else
    #   echo "PostgreSQL is already running."
    # fi
    #
    # # Set environment variables for PostgreSQL and Prisma
    # export PGHOST="localhost"
    # export PGPORT="2000"
    # export PGUSER="postgres"
    # export PGDATABASE="newlince"  # Replace with your actual database name

    pg_ctl stop
    export PGHOST="$PGDATA"
    [ ! -d $PGDATA ] && pg_ctl initdb -o "-U postgres"
    pg_ctl -o "-p 2000 -k $PGDATA" start

    echo "log_min_messages = warning" >> $PGDATA/postgresql.conf
    echo "log_checkpoints = off" >> $PGDATA/postgresql.conf

    # Start the application
    cd ${toString ./.}
    pnpm dev
  '';
}
