# Calendar exports from Karma simulation

Lince exports a Schedule Protein through an installed `.ics` plugin. Its worker requests the same projection and reads the same schedule and allocation rules as Time Castle. The clock keeps its own source and visible horizon. No clock needs to be open for exports to run.

## Use it

1. Run `cargo xtask plugin-files /absolute/path/to/lince-data`.
2. Restart Lince, open Sync Castle, and enable **Calendar and document exports**.
3. Create a Protein with source **Schedule**. Choose history days, future days, timezone, and conditions. For example, select the Shower Record and the next 30 days. Its daily Karma rule establishes future occurrences; a work estimate such as 10 minutes gives them duration. Schedule Proteins default to `#task` descending; change or remove the hierarchy as needed.
4. Run the Protein to see its occurrences, then save it.
5. In Sync Castle, choose **iCalendar (.ics)**. Enter an absolute directory, a filename such as `schedule.ics`, and a calendar name. Select the saved Protein and refresh interval.
6. Preview the selection and destination, then save and start Sync. If simulation is updating, preview again when it finishes.
7. Point a calendar app at the file using its file or subscription support. Importing a copy does not maintain that copy when Lince replaces the original.

Each Sync has a separate identity, Protein, settings, checkpoint, interval, directory, and filename. Several calendars can use the same plugin. Two Syncs cannot own the same output file, including directory aliases. **Markdown (.md)** exports a Record Protein through the same writer. The existing bidirectional Record directory Sync remains available separately.

| Instance | Source | Window | File |
|---|---|---|---|
| Personal calendar | Personal Schedule Protein | Next 30 days | `/home/me/calendars/personal/schedule.ics` |
| Work calendar | Work Schedule Protein | Next 90 days | `/home/me/calendars/work/schedule.ics` |
| Notes export | Notes Record Protein | Selected Records | `/home/me/notes/export/records.md` |

## Rolling windows

```json
{
  "source": "schedule",
  "where": [
    {
      "rolling_window": {
        "past_days": 0,
        "future_days": 30,
        "timezone": "America/Sao_Paulo"
      }
    },
    { "all": [{ "slug_eq": "shower" }] }
  ],
  "order": [{ "desc": "#task" }]
}
```

The window starts at local midnight, including chosen history, and ends at local midnight after the chosen future days. It rolls when the local date changes and accounts for daylight saving transitions. Fixed `projection_window` remains supported. One window belongs at the top level, outside condition groups.

The existing simulation budgets still apply. Document export supports at most 1,000 selected entries and a 512 KiB document. Narrow the Protein or shorten its window when the budget is exceeded. Record Protein limits are explicit selection policies. A truncated Schedule or incomplete simulation cannot replace the calendar.

The horizon bounds simulation. Conditions select its results; dependencies may require simulating other Records too. Projection uses a private snapshot and does not apply future quantities or actions to the live Cell.

## Calendar behavior

The plugin writes one `VEVENT` per occurrence, following [RFC 5545](https://www.rfc-editor.org/rfc/rfc5545.html). Its UID derives from the Cell and occurrence identity, so regenerating an occurrence or moving its proposed time keeps its identity. Timed entries use UTC instants and their estimated or allocated duration. Points have no invented duration. All-day ranges use an exclusive end date.

Projected events are tentative and transparent to free/busy reporting. There are no invitations or alarms. Forecasts change with live quantities, schedules, estimates, Karma rules or Protein choices. A daily reset establishes an occurrence; scheduling and allocation determine the proposed activity time.

Current unfinished tasks use the same moving cursor offsets as Time Castle, resolved when the export snapshot is taken. Previews approve the selected tasks and priorities; ordinary movement of the clock alone does not invalidate that approval.

The worker replaces the complete configured file atomically after a successful, complete projection. Updating, incomplete or failed simulations keep the last successful file. An updating projection is checked again on the worker's next short poll. Status reports the last successful export time after a failure.

Unchanged content avoids rewriting, but the worker checks the actual file so external edits are restored on the next successful refresh. An empty selection produces zero events and a UTC timezone component to keep the document valid. Events outside the rolling window leave the export; include history days to retain recent past events. Changing the destination leaves the old file in place.

File or calendar-app edits do not flow back into Lince. Replacement updates an app only if it reads or refreshes that source. Subscription support and refresh timing depend on the app.

## Plugin contract

A Sync contribution can declare `document` with a typed `source` (`record` or `schedule`) and filename `extension`. Document providers are outgoing only, declare required text settings `directory` and `filename`, and use an empty Record schema name because they import no Records.

The host sends `render` a bounded `ExportSnapshot` containing a Cell scope, generation timestamp, optional resolved window, and selected rows. The plugin returns `ExportDocument` with its text. The plugin converts the format; shared Sync infrastructure owns path validation, destination exclusivity, previews, atomic writing, status and retries. Ordinary protocol providers retain the Record `prepare`/`push` path.

Generation timestamps remain stable while selected content is unchanged. Checkpoints use each binding's existing private cursor. Plugin bookkeeping does not invalidate Karma's simulation cache; imported or edited live Records still do. Migration `0008_plugin_projection.sql` retires old bookkeeping triggers without changing any past migration.

## Verification

The nine format and Sync tests pass, including an actual daily Shower rule exporting three future occurrences without changing live quantity. They cover installation and plugin calls, independent destinations and horizons, external edits, incomplete simulations, empty calendars, stopped bindings, stale previews, path validation, symlinks, moving cursor times, escaping, and export budgets.

All 25 shared Time Castle tests pass after sharing the cursor calculation. The existing Protein, window, native draft model, Nostr, plugin host, Sync service, and migration guard tests also pass.

`cargo check -p store` and `cargo xtask check -p lince-desktop --lib` pass with workspace warnings denied.

The three native plugin-form tests pass. A subsequent native Time Castle test run was blocked by a concurrent map compile error in `crates/map/src/scene.rs`: borrowing `collection.features` after moving it. The shared Time Castle tests above pass on the shared cursor implementation.

```sh
cargo xtask test -p lince-plugin-files -p lince-interface --features models --test format --test sync --test time_castle
cargo xtask test -p nucleus --test rolling_window -p lince-interface --features models --test models -p protein --test schedule
cargo xtask test -p engine --test sync_service -p lince-plugin-nostr --test roundtrip -p lince-plugins --test host
cargo xtask test -p store --test migration_guard
cargo xtask test -p lince-desktop --lib sync_castle::plugins::tests
```
