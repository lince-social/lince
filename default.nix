{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  buildInputs = with pkgs; [
    postgresql_17
    nodejs_23
    prisma-engines
    prisma
  ];

  PGDATA = "${toString ./.}/.pg";
  # PRISMA_MIGRATION_ENGINE_BINARY = "${pkgs.prisma-engines}/bin/migration-engine";
  PRISMA_QUERY_ENGINE_BINARY = "${pkgs.prisma-engines}/bin/query-engine";
  PRISMA_QUERY_ENGINE_LIBRARY = "${pkgs.prisma-engines}/lib/libquery_engine.node";
  PRISMA_INTROSPECTION_ENGINE_BINARY = "${pkgs.prisma-engines}/bin/introspection-engine";
  PRISMA_FMT_BINARY = "${pkgs.prisma-engines}/bin/prisma-fmt";

  shellHook = ''
    # PATH="$PWD/node_modules/.bin:$PATH";


    pg_ctl stop
    export PGHOST="$PGDATA"
    [ ! -d $PGDATA ] && pg_ctl initdb -o "-U postgres"
    pg_ctl -o "-p 2000 -k $PGDATA" start
    echo "log_min_messages = warning" >> $PGDATA/postgresql.conf
    echo "log_checkpoints = off" >> $PGDATA/postgresql.conf

    cd ${toString ./.}
    npm run dev
  '';
}
