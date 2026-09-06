# Secrets at rest

Lince has no way to keep a secret. This was found while looking for somewhere to put a provider API key, and the gap is general: nothing in the database is the right shape or the right safety class for a credential, and the only working precedent lives outside it. Any feature that needs a key — a model provider, a third-party service, anything with a token — hits this same wall, so the answer is written here rather than inside whichever feature reached it first.

What exists and is not this. `store/src/logins.rs` is not credentials: a `Login` is `organ_uid`, `person_uid` and `created_at`, which binds a contact Organ to a Person and holds no secret. `identity_key` stores public keys only, keyed `(actor_uid, key_id)`, populated when a client publishes a key for a Person through the action-intent session and by transfer delivery. `configuration` is a typed singleton of UI and policy settings, unencrypted, and it is both the wrong shape and the wrong safety class for a key.

The one real precedent is `engine::trust::load_or_create_secret` at `crates/engine/src/trust.rs:60`: thirty-two raw bytes in a file created `0o600`, deliberately not in the database, with the node key kept distinct from the identity key. That is the shape to follow, and the reason is what a Record would do instead — a secret in a Record is a secret in the op log, replicated to every grantee, kept forever, and carried in any backup of the database.

Karma already has a hole shaped like this feature. `InputSource::SecretMetadata { secret }` exists in the AST at `crates/nucleus/src/karma/ast.rs:180` with no secret store anywhere behind it, so a rule can name a secret that cannot exist.

- [ ] Build the secret store as `0600` files under the Cell's config directory, one per secret, following the node-secret precedent — never in a Record, never in the op log, never in `configuration`, and therefore never synced to another Organ or carried in a database backup.
- [ ] Reach it only through a narrow host call that writes a value and reports whether one is present and whether it works, so no surface ever reads a secret back.
- [ ] Keep environment variables supported for the developer path, and say which source a given secret is coming from, or "it works on my machine and not in the app" is unanswerable.
- [ ] Give `InputSource::SecretMetadata` something real to resolve against, or say in the Karma surface that it resolves to nothing.
