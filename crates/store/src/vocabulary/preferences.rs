// non-lean-yet
// Scope: fn terminology_owner, fn terminology_context_on, fn set_terminology_context_on, fn
//   terminology_state_on.
// BeautifulRust (baseline lines):
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   hide_term_on:hidden (178).
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 10, 23, 42, 103, 111, 147, 174, 218.
// - [Clippy: clippy::needless_question_mark] enclosing `Ok` and `?` operator are unneeded. Sites
//   94.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 10,
//   23, 42, 103, 111, 147, 174, 218.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{StoreError, invalid, term_text};

pub fn terminology_owner(person: Option<&str>) -> &str {
    person.unwrap_or("local")
}

pub async fn terminology_context_on(
    connection: &mut SqliteConnection,
    owner: &str,
) -> Result<String, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT context FROM terminology_context WHERE owner_key=?")
            .bind(owner)
            .fetch_optional(connection)
            .await?
            .unwrap_or_else(|| "General".into()),
    )
}

pub async fn set_terminology_context_on(
    connection: &mut SqliteConnection,
    owner: &str,
    context: &str,
) -> Result<(), StoreError> {
    let context = term_text(context)?;
    if context.chars().count() > 64 {
        return Err(invalid("Use a context name of at most 64 characters"));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT context FROM terminology_choice WHERE owner_key=? UNION SELECT context FROM terminology_hidden WHERE owner_key=? UNION SELECT context FROM terminology_context WHERE owner_key=?) WHERE context!=?")
        .bind(owner).bind(owner).bind(owner).bind(&context).fetch_one(&mut *connection).await?;
    if count >= 32 {
        return Err(invalid("Use at most 32 terminology contexts"));
    }
    sqlx::query("INSERT INTO terminology_context(owner_key,context) VALUES(?,?) ON CONFLICT(owner_key) DO UPDATE SET context=excluded.context")
        .bind(owner).bind(context).execute(connection).await?;
    Ok(())
}

pub async fn terminology_state_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
) -> Result<Value, StoreError> {
    let owner = terminology_owner(person);
    let context = terminology_context_on(connection, owner).await?;
    let choices = {
        let mut query = choices_query(owner, &context, "concept_uid,term_uid,wording,original");
        query
            .build()
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(|row| {
                Value::from_iter([
                    ("concept_uid", Value::String(row.get("concept_uid"))),
                    ("term_uid", row.get::<Option<String>, _>("term_uid").into()),
                    ("wording", row.get::<Option<String>, _>("wording").into()),
                    ("original", row.get::<bool, _>("original").into()),
                ])
            })
            .collect()
    };
    let hidden = hidden_terms_on(connection, owner, &context).await?;
    Ok(Value::from_iter([
        ("context", context.into()),
        ("choices", Value::Array(choices)),
        ("hidden_terms", hidden.into()),
    ]))
}

fn choices_query(owner: &str, context: &str, columns: &str) -> sqlx::QueryBuilder<sqlx::Sqlite> {
    let mut query = sqlx::QueryBuilder::new("SELECT ");
    query
        .push(columns)
        .push(" FROM terminology_choice WHERE owner_key=")
        .push_bind(owner)
        .push(" AND context=")
        .push_bind(context)
        .push(" ORDER BY concept_uid");
    query
}

async fn hidden_terms_on(
    connection: &mut SqliteConnection,
    owner: &str,
    context: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT term_uid FROM terminology_hidden WHERE owner_key=? AND context=? ORDER BY term_uid",
    )
    .bind(owner)
    .bind(context)
    .fetch_all(connection)
    .await?)
}

pub async fn terminology_state(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<Value, StoreError> {
    let mut connection = pool.acquire().await?;
    terminology_state_on(&mut connection, person).await
}

pub async fn choose_term_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
    concept: &str,
    term: Option<&str>,
) -> Result<(), StoreError> {
    let owner = terminology_owner(person);
    let context = terminology_context_on(connection, owner).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
            .bind(concept)
            .fetch_one(&mut *connection)
            .await?;
    if !exists {
        return Err(invalid("Choose a Concept with a saved source meaning"));
    }
    let wording: Option<String> = if let Some(term) = term {
        Some(sqlx::query_scalar("SELECT name FROM concept_term WHERE uid=? AND concept_uid=? AND retired=0 AND (owner_uid IS NULL OR owner_uid=?)")
            .bind(term).bind(concept).bind(person).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Choose a visible term of this Concept"))?)
    } else {
        None
    };
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM terminology_choice WHERE owner_key=? AND context=? AND concept_uid!=?")
        .bind(owner).bind(&context).bind(concept).fetch_one(&mut *connection).await?;
    if count >= 4096 {
        return Err(invalid("Use at most 4096 choices in a terminology context"));
    }
    sqlx::query("INSERT INTO terminology_choice(owner_key,context,concept_uid,term_uid,wording,original) VALUES(?,?,?,?,?,?) ON CONFLICT(owner_key,context,concept_uid) DO UPDATE SET term_uid=excluded.term_uid,wording=excluded.wording,original=excluded.original")
        .bind(owner).bind(&context).bind(concept).bind(term).bind(wording).bind(term.is_none()).execute(&mut *connection).await?;
    if term.is_none() {
        sqlx::query("DELETE FROM terminology_hidden WHERE owner_key=? AND context=? AND term_uid IN (SELECT uid FROM concept_term WHERE concept_uid=?)")
            .bind(owner).bind(context).bind(concept).execute(connection).await?;
    }
    Ok(())
}

pub async fn use_wording_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
    concept: &str,
    wording: &str,
) -> Result<(), StoreError> {
    let wording = term_text(wording)?;
    let owner = terminology_owner(person);
    let context = terminology_context_on(connection, owner).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
            .bind(concept)
            .fetch_one(&mut *connection)
            .await?;
    if !exists {
        return Err(invalid("Choose a Concept with a saved source meaning"));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM terminology_choice WHERE owner_key=? AND context=? AND concept_uid!=?")
        .bind(owner).bind(&context).bind(concept).fetch_one(&mut *connection).await?;
    if count >= 4096 {
        return Err(invalid("Use at most 4096 choices in a terminology context"));
    }
    sqlx::query("INSERT INTO terminology_choice(owner_key,context,concept_uid,wording,original) VALUES(?,?,?,?,0) ON CONFLICT(owner_key,context,concept_uid) DO UPDATE SET term_uid=NULL,wording=excluded.wording,original=0")
        .bind(owner).bind(context).bind(concept).bind(wording).execute(connection).await?;
    Ok(())
}

pub async fn hide_term_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
    term: &str,
    hidden: bool,
) -> Result<(), StoreError> {
    let owner = terminology_owner(person);
    let context = terminology_context_on(connection, owner).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM concept_term WHERE uid=? AND owner_uid IS NULL)",
    )
    .bind(term)
    .fetch_one(&mut *connection)
    .await?;
    if !exists {
        return Err(invalid("Choose a shared source term to hide or show"));
    }
    if hidden {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM terminology_hidden WHERE owner_key=? AND context=? AND term_uid!=?")
            .bind(owner).bind(&context).bind(term).fetch_one(&mut *connection).await?;
        if count >= 8192 {
            return Err(invalid("Hide at most 8192 terms in a terminology context"));
        }
        sqlx::query(
            "INSERT OR IGNORE INTO terminology_hidden(owner_key,context,term_uid) VALUES(?,?,?)",
        )
        .bind(owner)
        .bind(context)
        .bind(term)
        .execute(connection)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM terminology_hidden WHERE owner_key=? AND context=? AND term_uid=?",
        )
        .bind(owner)
        .bind(context)
        .bind(term)
        .execute(connection)
        .await?;
    }
    Ok(())
}

pub async fn reset_terminology_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
    concept: &str,
) -> Result<(), StoreError> {
    let owner = terminology_owner(person);
    let context = terminology_context_on(connection, owner).await?;
    sqlx::query("DELETE FROM terminology_choice WHERE owner_key=? AND context=? AND concept_uid=?")
        .bind(owner)
        .bind(&context)
        .bind(concept)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM terminology_hidden WHERE owner_key=? AND context=? AND term_uid IN (SELECT uid FROM concept_term WHERE concept_uid=?)")
        .bind(owner).bind(context).bind(concept).execute(connection).await?;
    Ok(())
}

pub(super) async fn preferences(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<(HashMap<String, Option<String>>, HashSet<String>), StoreError> {
    let _preferences = nucleus::performance::Span::new("store-terminology-preferences-await");
    let mut connection = pool.acquire().await?;
    let owner = terminology_owner(person);
    let context = terminology_context_on(&mut connection, owner).await?;
    let choices = {
        let mut query = choices_query(owner, &context, "concept_uid,wording");
        query
            .build_query_as::<(String, Option<String>)>()
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .collect()
    };
    let hidden = hidden_terms_on(&mut connection, owner, &context)
        .await?
        .into_iter()
        .collect();
    Ok((choices, hidden))
}
