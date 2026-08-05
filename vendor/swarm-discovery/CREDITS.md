# Vendored dependency: swarm-discovery

- Project: [rkuhn/swarm-discovery](https://github.com/rkuhn/swarm-discovery)
- Version: 0.6.3
- Copyright: Roland Kuhn and contributors
- License: Apache-2.0; the complete license text is bundled as
  `LICENSE.Apache_2.0`.

Lince carries one local change in `src/socket.rs`: it does not enable
`SO_REUSEPORT` for mDNS sockets. This prevents Linux from load-balancing
multicast datagrams among unrelated local applications, which can otherwise
stop a Lince Cell from receiving LAN discovery announcements.
