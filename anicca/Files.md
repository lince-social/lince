# Files in messages

Lince cannot carry bytes today. Media is host-local, served from `/host/media` by `presentation/http/media_assets.rs`, and the sync path carries ops and Loro snapshots and nothing else. So a file cannot travel to another Organ at all, and several features have been planning around that as though it were permanent.

It is not permanent, and most of the work already ships with the transport Lince uses. `iroh` at `=1.0.3` is already a workspace dependency, and `iroh-blobs` at `0.103.0` — content-addressed blobs, BLAKE3, with a filesystem store behind its `fs-store` feature — was verified rather than assumed to resolve alongside it into one lockfile with a single `iroh 1.0.3`.

The design that follows is small. An attachment is a tiny op on a Message Record: hash, filename, size and mime type, a handful of bytes, so the op log, the Ledger and every contact's feed stay exactly as small as they are today and nothing about sync's shape changes. The bytes move out of band over the iroh connection the two Organs already have, fetched when somebody clicks. Content addressing means a file received twice is stored once and a corrupted transfer is detectable rather than merely suspicious.

Rejected outright: base64 in a Record body or in an extension. It works today with no new dependency, and it puts a megabyte of payload into an op that is replicated to every grantee and kept forever. A Command that stuffs file contents into Record bodies is the same trade wearing a different shape.

The honest failure case has to be visible. Fetch-on-demand means a file whose sender is offline cannot be retrieved right now. That is the same property the sealed mailbox has and it needs the same treatment.

This does not change how code travels. A repository is not an attachment and git is better at moving it, so reference-and-patch stays right for code — that is in `anicca/Code.md`. What this retires is "Lince cannot carry bytes": an image, a PDF or a design file in a conversation becomes ordinary rather than blocked.

- [ ] Put files in messages over `iroh-blobs`: the hash rides the Record as an op, the bytes ride the iroh connection, fetched on click.
- [ ] Say out loud when a file cannot be fetched because its sender is not reachable — never a spinner that looks like corruption, never an error that looks like the file is gone.
- [ ] Build the surface it was asked for: choose an Organ, attach, send, and on the other side a Message with a file on it and a button.
