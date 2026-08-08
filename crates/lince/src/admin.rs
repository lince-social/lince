//! Configuring a Cell that has no board.
//!
//! `--server` removes the UI, and the UI was the only place three facts could
//! be set: whether the discovery door is open (`lince.discovery.accept_unknown`),
//! whether a contact is `known`, and which Person a contact's live session acts
//! as (`organ_login`). None of those are derivable — they are decisions — so a
//! headless Cell needs somewhere to be told them, or `--server` ships a box
//! that can hold data and can never be reached.
//!
//! These are DELIBERATELY not HTTP routes. Every one of them is the bootstrap
//! that precedes having any credential on the box, so putting them behind the
//! credential they create is a circle; and they are administration of the
//! machine, which is what shell access already means. They read and write the
//! same store the server does, so run them as the same user (`sudo -u lince
//! lince --data-dir /var/lib/lince organ list`) and expect SQLite's write lock
//! to serialise them against a running server.

use std::io::Error;

use store::Store;

/// Dispatch an admin subcommand, or `None` when the args are a normal boot.
///
/// Returning `Option` rather than exiting keeps `main` in charge of the
/// process: an unknown first argument is a boot with flags, not an error.
pub async fn dispatch(args: &[String]) -> Option<Result<(), Error>> {
    let verbs = positional(args);
    match verbs.split_first() {
        Some((&"organ", rest)) => Some(organ(rest).await),
        Some((&"discovery", rest)) => Some(discovery(rest).await),
        _ => None,
    }
}

/// Flags that consume the token after them.
///
/// `lince --data-dir /var/lib/lince organ list` has to reach `organ`, and a
/// naive "first non-flag argument" reads `/var/lib/lince` as the subcommand —
/// which then silently falls through to a normal boot with a confusing error.
const VALUE_FLAGS: [&str; 5] = [
    "--data-dir",
    "--port",
    "--listen-addr",
    "--initial-admin-password",
    "--initial-admin-password-file",
];

fn positional(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        if VALUE_FLAGS.contains(&arg.as_str()) {
            rest.next();
        } else if !arg.starts_with('-') {
            out.push(arg.as_str());
        }
    }
    out
}

async fn open() -> Result<Store, Error> {
    Store::open(&web::default_lince_db_url())
        .await
        .map_err(|error| Error::other(format!("Cannot open the store: {error}")))
}

fn oops(message: impl Into<String>) -> Error {
    Error::other(message.into())
}

/// Resolve a contact from a uid or any unambiguous prefix of one.
///
/// Uids are long and are read off a screen; an ambiguous prefix is an error
/// rather than a pick, because the two things a prefix could name here are
/// "someone I meant to trust" and "someone else".
async fn resolve(store: &Store, needle: &str) -> Result<store::organs::Contact, Error> {
    let contacts = store::organs::contacts(&store.pool)
        .await
        .map_err(|error| oops(error.to_string()))?;
    let hits: Vec<_> = contacts
        .into_iter()
        .filter(|contact| {
            contact.record_uid == needle
                || contact.record_uid.starts_with(needle)
                || contact.head.eq_ignore_ascii_case(needle)
        })
        .collect();
    match hits.len() {
        1 => Ok(hits.into_iter().next().expect("one")),
        0 => Err(oops(format!(
            "No contact matches `{needle}`. `lince organ list` shows what there is."
        ))),
        _ => Err(oops(format!(
            "`{needle}` matches {} contacts. Use more of the uid.",
            hits.len()
        ))),
    }
}

async fn organ(verbs: &[&str]) -> Result<(), Error> {
    let store = open().await?;
    match verbs {
        [] | ["list"] => {
            let contacts = store::organs::contacts(&store.pool)
                .await
                .map_err(|error| oops(error.to_string()))?;
            if contacts.is_empty() {
                println!("No contacts yet.");
                println!(
                    "Someone pairs with this Cell from their board; that needs the \
                     discovery door open (`lince discovery accept-unknown on`)."
                );
                return Ok(());
            }
            println!("{:<40}  {:<10}  {:<20}  NAME", "ORGAN UID", "TRUST", "LOGIN AS");
            for contact in contacts {
                // The login is the interesting column: `known` alone opens
                // sync, and live mode additionally needs this to be set.
                let login = match store::logins::person_for_organ(&store.pool, &contact.record_uid)
                    .await
                    .map_err(|error| oops(error.to_string()))?
                {
                    Some(person) => store::records::get(&store.pool, &person)
                        .await
                        .ok()
                        .flatten()
                        .map(|record| record.head)
                        .unwrap_or(person),
                    None => "-".to_string(),
                };
                println!(
                    "{:<40}  {:<10}  {:<20}  {}",
                    contact.record_uid, contact.trust, login, contact.head
                );
            }
            Ok(())
        }
        ["trust", needle, level @ ("known" | "unknown" | "blocked")] => {
            let contact = resolve(&store, needle).await?;
            store::organs::set_trust(&store.pool, &contact.record_uid, level)
                .await
                .map_err(|error| oops(error.to_string()))?;
            println!("{} is now `{level}`.", contact.head);
            if *level == "known" {
                println!(
                    "That opens sync. For live mode they also need a login: \
                     `lince organ login {} <username>`.",
                    &contact.record_uid
                );
            }
            Ok(())
        }
        ["trust", _, level] => Err(oops(format!(
            "`{level}` is not a trust level. Use known, unknown or blocked."
        ))),
        ["login", needle, username] => {
            let contact = resolve(&store, needle).await?;
            // `known` is not implied. A login says which Person they act as;
            // it does not say they may connect, and quietly granting both from
            // one command would make the narrower thing unavailable.
            if contact.trust != "known" {
                return Err(oops(format!(
                    "{} is `{}`, and only a known Organ may open a live session. \
                     Run `lince organ trust {} known` first.",
                    contact.head, contact.trust, contact.record_uid
                )));
            }
            let user = store::auth::user_by_username(&store.pool, username.trim())
                .await
                .map_err(|error| oops(error.to_string()))?
                .ok_or_else(|| {
                    oops(format!(
                        "No user `{username}` on this Cell. `lince organ users` lists them."
                    ))
                })?;
            store::logins::grant(&store.pool, &contact.record_uid, &user.uid)
                .await
                .map_err(|error| oops(error.to_string()))?;
            println!(
                "{} may now enter this Lince as `{}`.",
                contact.head, user.username
            );
            Ok(())
        }
        ["logout", needle] => {
            let contact = resolve(&store, needle).await?;
            store::logins::revoke(&store.pool, &contact.record_uid)
                .await
                .map_err(|error| oops(error.to_string()))?;
            println!("{} can no longer enter this Lince.", contact.head);
            Ok(())
        }
        ["users"] => {
            let users = store::auth::list_users(&store.pool)
                .await
                .map_err(|error| oops(error.to_string()))?;
            if users.is_empty() {
                println!("No users with a login on this Cell.");
                return Ok(());
            }
            for (_uid, username, name, role) in users {
                println!("{username:<24}  {role:<16}  {name}");
            }
            Ok(())
        }
        _ => Err(oops(ORGAN_USAGE.to_string())),
    }
}

const ORGAN_USAGE: &str = "Usage:
  lince organ list
  lince organ users
  lince organ trust <uid-or-name> <known|unknown|blocked>
  lince organ login <uid-or-name> <username>
  lince organ logout <uid-or-name>";

const DISCOVERY_USAGE: &str = "Usage:
  lince discovery
  lince discovery <accept-unknown|accept-logins|local|internet> <on|off>";

async fn discovery(verbs: &[&str]) -> Result<(), Error> {
    let store = open().await?;
    let organ = store::organs::local(&store.pool)
        .await
        .map_err(|error| oops(error.to_string()))?
        .ok_or_else(|| oops("This Cell has no local Organ yet; start it once first."))?;
    let current = store::records::get_extension(&store.pool, &organ.uid, "lince.discovery")
        .await
        .map_err(|error| oops(error.to_string()))?
        .unwrap_or_else(|| serde_json::json!({}));

    // Defaults live in `web::discovery_is_local` / `discovery_reaches_internet`
    // and `Wire::accept_unknown`, and differ per key. Mirror them rather than
    // inventing a single default, so what this prints is what the Cell does.
    let read = |key: &str, default: bool| {
        current
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default)
    };

    match verbs {
        [] | ["show"] => {
            println!("local           {}   mDNS on this network", onoff(read("local", true)));
            println!(
                "internet        {}   reachable off-LAN",
                onoff(read("internet", true))
            );
            println!(
                "accept-unknown  {}   strangers may introduce themselves (pairing)",
                onoff(read("accept_unknown", false))
            );
            println!(
                "accept-logins   {}   anyone with a username and password may enter",
                onoff(read("accept_logins", true))
            );
            Ok(())
        }
        [key @ ("accept-unknown" | "accept-logins" | "local" | "internet"), value @ ("on" | "off")] => {
            let field = key.replace('-', "_");
            let mut next = current.clone();
            next.as_object_mut()
                .ok_or_else(|| oops("lince.discovery is not an object"))?
                .insert(field.clone(), serde_json::Value::Bool(*value == "on"));
            store::records::set_extension(&store.pool, &organ.uid, "lince.discovery", &next)
                .await
                .map_err(|error| oops(error.to_string()))?;
            println!("discovery.{field} = {value}");
            if field == "accept_unknown" || field == "accept_logins" {
                // Read per connection in `serve_connection`, unlike `local`
                // and `internet`, which are Endpoint builder options fixed at
                // bind. Saying which take effect now is the difference between
                // a working pair attempt and a confusing one.
                println!("In effect immediately — no restart needed.");
                if *value == "on" {
                    println!(
                        "Anyone who can reach this endpoint may now introduce themselves. \
                         They land as `unknown` and get nothing until you trust them; \
                         turn this back off once you have paired."
                    );
                }
            } else {
                println!("Takes effect when the Cell rebinds its endpoint.");
            }
            Ok(())
        }
        _ => Err(oops(DISCOVERY_USAGE.to_string())),
    }
}

fn onoff(value: bool) -> &'static str {
    if value { "on " } else { "off" }
}
