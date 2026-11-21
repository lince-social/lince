// non-lean-yet
// Scope: struct Cancellation, struct Backup, fn failed, fn copy_pages.
// BeautifulRust (baseline lines):
// - [review: §8 Unsafe] Check local SAFETY proof against the operation. Sites 36, 43, 61, 71, 79,
//   86.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 91.
// - [Clippy: clippy::cast_sign_loss] Prove the sign or preserve behavior with an explicit
//   conversion. Sites 91.
// - [Clippy: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones. Sites
//   105.

use std::{
    path::Path,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use libsqlite3_sys::{
    SQLITE_DONE, SQLITE_OK, sqlite3_backup, sqlite3_backup_finish, sqlite3_backup_init,
    sqlite3_backup_pagecount, sqlite3_backup_step, sqlite3_errcode,
};
use sqlx::{
    Connection, SqliteConnection, SqlitePool,
    sqlite::{LockedSqliteHandle, SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};

use crate::StoreError;

struct Cancellation(Arc<AtomicBool>);

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct Backup(NonNull<sqlite3_backup>);

impl Backup {
    fn finish(self) -> i32 {
        let handle = self.0;
        std::mem::forget(self);
        // SAFETY: `handle` is still live, and forgetting `self` prevents a second finish in Drop.
        unsafe { sqlite3_backup_finish(handle.as_ptr()) }
    }
}

impl Drop for Backup {
    fn drop(&mut self) {
        // SAFETY: This is the unfinished handle created by sqlite3_backup_init.
        unsafe {
            sqlite3_backup_finish(self.0.as_ptr());
        }
    }
}

fn failed(code: i32) -> StoreError {
    StoreError::Protocol(format!("database snapshot failed with SQLite code {code}"))
}

fn copy_pages(
    source: &mut LockedSqliteHandle<'_>,
    destination: &mut LockedSqliteHandle<'_>,
    cancelled: &AtomicBool,
) -> Result<(), StoreError> {
    let source = source.as_raw_handle();
    let destination = destination.as_raw_handle();
    // SAFETY: The distinct connections stay locked for the whole copy and both names are C strings.
    let handle = unsafe {
        sqlite3_backup_init(
            destination.as_ptr(),
            c"main".as_ptr(),
            source.as_ptr(),
            c"main".as_ptr(),
        )
    };
    let backup = Backup(NonNull::new(handle).ok_or_else(|| {
        // SAFETY: The destination remains locked, and the failed init left no live backup.
        let code = unsafe { sqlite3_errcode(destination.as_ptr()) };
        failed(code)
    })?);
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(StoreError::Protocol("database snapshot cancelled".into()));
        }
        // SAFETY: `backup` owns the live handle and both connections are still locked.
        match unsafe { sqlite3_backup_step(backup.0.as_ptr(), 256) } {
            SQLITE_DONE => break,
            SQLITE_OK => {}
            code => return Err(failed(code)),
        }
    }
    // SAFETY: The successful step completed the copy without freeing the handle.
    let pages = unsafe { sqlite3_backup_pagecount(backup.0.as_ptr()) };
    let code = backup.finish();
    if code != SQLITE_OK {
        return Err(failed(code));
    }
    nucleus::performance::add("store-snapshot-pages", pages.max(0) as u64);
    Ok(())
}

pub(super) async fn snapshot(pool: &SqlitePool, path: &Path) -> Result<(), StoreError> {
    path.to_str()
        .ok_or_else(|| StoreError::Protocol("snapshot path is not UTF-8".into()))?;
    let source = pool.acquire().await?;
    let reserved = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let path = path.to_owned();
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancellation = Cancellation(cancelled.clone());
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        runtime.block_on(async move {
            let _work = nucleus::performance::Span::new("store-snapshot-copy-worker-await");
            let mut source = source;
            let options = SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .journal_mode(SqliteJournalMode::Delete)
                .synchronous(SqliteSynchronous::Full);
            let mut destination = SqliteConnection::connect_with(&options).await?;
            let outcome = async {
                let mut transaction = source.begin().await?;
                let copied = async {
                    sqlx::query("SELECT rootpage FROM sqlite_schema LIMIT 1")
                        .fetch_optional(&mut *transaction)
                        .await?;
                    let mut source = transaction.lock_handle().await?;
                    let mut destination = destination.lock_handle().await?;
                    copy_pages(&mut source, &mut destination, &cancelled)
                }
                .await;
                let released = transaction.rollback().await;
                copied?;
                released?;
                Ok::<_, StoreError>(())
            }
            .await;
            let closed = destination.close().await;
            outcome?;
            closed?;
            reserved.sync_all()?;
            Ok(())
        })
    })
    .await
    .map_err(|error| StoreError::Protocol(format!("database snapshot worker failed: {error}")))?
}
