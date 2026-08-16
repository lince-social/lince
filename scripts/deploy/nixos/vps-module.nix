# Both relay jobs on ONE VPS, kept apart (Ontology §11, cluster C4).
#
# The two things an operator with a spare box is told to run are an
# `iroh-relay` — a rendezvous and fallback for connections, not Lince at all —
# and a Lince relay Cell, which carries gossip and authors nothing. Running
# both on one machine is normal and cheap. Running them as one thing is the
# mistake this module exists to make impossible.
#
# Importing this module gets you both, plus the assertions below. It adds no
# options of its own: everything is configured through `services.lince` and
# `services.iroh-relay` exactly as if they had been imported separately, and
# the only thing gained is that a configuration which BLURS them stops being
# deployable instead of merely being a bad idea.
#
# Why assertions rather than a comment saying "use different users": the
# separation is already structural — the two units declare different system
# users, different `StateDirectory` values, and the relay runs under
# `ProtectHome`, which is what keeps it away from a Cell store in a home
# directory. Structural separation that nothing checks is separation that
# survives until the first person sets `services.iroh-relay.user = "lince"`
# because it seemed tidier.
{
  config,
  lib,
  ...
}:

let
  lince = config.services.lince;
  relay = config.services.iroh-relay;
  both = lince.enable && relay.enable;

  # A bind address is `host:port`; only the port can collide, and only when
  # both are actually listening. `lib.last (splitString ":")` handles a bare
  # IPv4 host:port, which is the documented form for both options.
  portOf = address: lib.last (lib.splitString ":" address);
  lincePort = portOf lince.listenAddr;
  relayPorts = [
    (portOf relay.httpBindAddr)
  ]
  ++ lib.optional (relay.hostname != null) (portOf relay.httpsBindAddr);
in
{
  imports = [
    ./lince-module.nix
    ./iroh-relay-module.nix
  ];

  config.assertions = lib.mkIf both [
    {
      # The one that matters most: a shared user account is a shared filesystem
      # identity, and the relay's whole security story is that a break-in there
      # reaches nothing of yours.
      assertion = relay.user != lince.user;
      message = ''
        services.iroh-relay.user and services.lince.user are both
        "${relay.user}". An iroh relay is not Lince — it is a dependency Lince
        can use — and sharing a user account undoes the separation the two
        modules are built around: the relay could then read the Cell's store,
        which is plaintext at rest by design.

        Leave both at their defaults ("iroh-relay" and "lince") unless you have
        a reason, and then give them different names.
      '';
    }
    {
      assertion = relay.group != lince.group;
      message = ''
        services.iroh-relay.group and services.lince.group are both
        "${relay.group}". A shared group is a shared read permission on the
        Cell's data directory, which is the thing the separate users exist to
        prevent.
      '';
    }
    {
      # Not hypothetical: `services.lince.listenAddr` is operator-set and its
      # documentation says to put a TLS proxy in front of it, which is exactly
      # the reasoning that leads someone to 0.0.0.0:443 — the port the relay
      # is already on.
      assertion = !(lib.elem lincePort relayPorts);
      message = ''
        services.lince.listenAddr (${lince.listenAddr}) and an
        services.iroh-relay bind address are both on port ${lincePort}. One of
        the two units will fail to start, and which one is a race.

        These are separate processes on one machine by design. Give the Cell
        its own port and reverse-proxy to it if it needs to be on 443.
      '';
    }
    {
      # `dataDir` under the relay's own state directory would put a Cell store
      # somewhere a compromised relay can reach, which is the blurring the
      # design calls out by name: a box that holds plaintext "just to help" is
      # a full Cell with none of a full Cell's accountability.
      assertion = !(lib.hasPrefix "/var/lib/iroh-relay" lince.dataDir);
      message = ''
        services.lince.dataDir (${lince.dataDir}) is inside the iroh relay's
        state directory. The Cell's store is plaintext at rest, and the relay
        must not be able to reach it.

        The choice is readable-and-yours, or unreadable-and-anyone's. There is
        no middle option, and a shared directory is an attempt at one.
      '';
    }
  ];
}
