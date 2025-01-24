{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell { buildInputs = with pkgs; [ postgresql_17 ];

  PGDATA = "${toString ./.}/.pg";

  shellHook = ''

    pg_ctl stop
    export PGHOST="$PGDATA"
    [ ! -d $PGDATA ] && pg_ctl initdb -o "-U postgres"
    pg_ctl -o "-p 2000 -k $PGDATA" start
    echo "log_min_messages = warning" >> $PGDATA/postgresql.conf
    echo "log_checkpoints = off" >> $PGDATA/postgresql.conf

    [[ ! -d "$HOME/.bun" ]] && curl -fsSL https://bun.sh/install | bash && export BUN_INSTALL="$HOME/.bun" && export PATH="$BUN_INSTALL/bin:$PATH"

    cd ${toString ./.}
    bun run db/startup.ts
    bun run src/app.tsx
  '';
}
