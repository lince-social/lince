use std::io::Error;

use store::Store;

pub async fn dispatch(args: &[String]) -> Option<Result<(), Error>> {
    let verbs = positional(args);
    match verbs.split_first() {
        Some((&"organ", rest)) => Some(organ(rest).await),
        Some((&"discovery", rest)) => Some(discovery(rest).await),
        _ => None,
    }
}

const VALUE_FLAGS: [&str; 5] = [
    "--directory",
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
    Store::open(&cell::default_lince_db_url()?)
        .await
        .map_err(|error| Error::other(format!("Cannot open the store: {error}")))
}

fn oops(message: impl Into<String>) -> Error {
    Error::other(message.into())
}

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
            println!(
                "{:<40}  {:<10}  {:<20}  NAME",
                "ORGAN UID", "TRUST", "LOGIN AS"
            );
            for contact in contacts {
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
            if !store::people::is_active(&store.pool, &user.uid)
                .await
                .map_err(|error| oops(error.to_string()))?
            {
                return Err(oops(format!(
                    "`{username}` is deactivated on this Cell. Reactivate them in \
                     Roles & Permissions first."
                )));
            }
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
            for (uid, username, name, role) in users {
                let standing = match store::people::is_active(&store.pool, &uid).await {
                    Ok(true) => "",
                    Ok(false) => "  (deactivated)",
                    Err(_) => "  (standing unreadable)",
                };
                println!("{username:<24}  {role:<16}  {name}{standing}");
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

    let read = |key: &str, default: bool| {
        current
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default)
    };

    match verbs {
        [] | ["show"] => {
            println!(
                "local           {}   mDNS on this network",
                onoff(read("local", true))
            );
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
        [
            key @ ("accept-unknown" | "accept-logins" | "local" | "internet"),
            value @ ("on" | "off"),
        ] => {
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
