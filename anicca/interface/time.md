# Time in the v1 interface

Owner source: [Interface in Lince](../Lince.lingua), updated 2026-09-06.
Status: planned v1 work. The [Interface plan](plans/interface.md) owns the order
and completion checks. These surfaces are not claimed as implemented.

## Bevy implementation

Build Calendar and Clock directly as Bevy Sands: ordinary UI/text controls,
Bevy curves and meshes for the timeline/spiral, and Bevy cameras for its views.
A custom Bevy plugin can own the temporal layout without introducing a second
rendering API or a physics dependency. Use Bevy scheduling for presentation
updates, but Lince's domain clock and occurrence projection remain authoritative
for dates and recurrence. Sleep until an actual visible-time deadline or data
change; an idle agenda does not need a permanent animation loop.

## Calendar and timeline

A Calendar is a Castle made of ordinary navigation, date, selection and
configuration Sands around a specialized native calendar/timeline Sand. It
accepts time-related Record properties directly. Its first implementation
does not depend on time being expressed through Areas or on moving Box bodies.

The person chooses a Protein and maps the title, Record reference, start,
end or due date. An instant appears as a point; an interval occupies a strip.
A date without an hour appears in an all-day lane, never at an invented
midnight appointment. Missing dates appear in an unscheduled list. Invalid
dates or an end before a start show the offending item and the reason.

Start with an agenda and a day/week calendar over one bounded visible period.
Previous, next and today controls, a date picker and an explicit timezone are
ordinary Sands. A linear timeline uses the same entries and selection. Open
the source Record from any appearance. Overlaps use separate lanes or an
expandable stack. Large results are paged with a visible count or limit.

The input keeps source uid, mapped date-field identity, occurrence identity
when repeated, precision, interval and timezone. These identities survive
period, order and camera changes. Repeated appearances never create Records.

Recurring entries come from the authoritative Karma/Frequency occurrence
projection for the requested period. The frontend never implements another
recurrence calculator or fires a Rule by drawing an occurrence. Bound result
count and evaluation work. Timezones, daylight-saving transitions, missed
occurrences and schedule edits follow the domain's policies. If the domain
cannot supply one of these, expose the missing capability and finish that
dependency before claiming recurrence support.

Date edits use the mapped domain Action. Dragging an interval previews the
dates to submit; a keyboard editor provides the same operation. An occurrence
edit says whether it changes one occurrence or its series. Offer only
operations the source supports; otherwise open the Record or schedule editor
with an explanation. Failed writes retain the draft and show the refusal.

The existing Protein Timeline source summarizes quantities and contributors;
it is not a complete calendar-entry contract. Reuse Record/Promise dates and
occurrence machinery, adding missing read and Action bindings with the UI.
Do not turn aggregate buckets into appointments.

## Clock and spiral

The Clock is another Castle over the same entries. Its ordinary top view
shows the rolling next hour and a clear Now mark. Perspective opens the strip
into a spiral and shows a configurable finite future horizon. One turn means
one hour; height advances with elapsed time. These initial layout choices
remain visible settings.

A point occupies its place on the strip; an interval spans its duration.
An interval crossing a turn has connected segments with one identity. Each
recurrence occurrence has its own appearance. Date-only and unscheduled work
stays in labeled companion lists because it has no minute position. Coincident
entries remain selectable through a stack or the accompanying agenda.

Top view explicitly filters presentation to the next hour. A plain overhead
projection would stack later turns on top of it, so geometry alone cannot
provide this meaning. Perspective reveals and labels the configured horizon.
Camera changes never change occurrences, reminders or Rule execution. Keep
the selected occurrence, indicating when it lies outside the top window.

An interval already in progress is clipped to the visible period with its true
start available on inspection. Unfinished overdue work remains in an Overdue
list when it leaves the live strip. Moving Now must not make it appear complete.

Live mode advances Now from Lince's clock. A person can hold the displayed
time for inspection, with a Held time label and Return to now. Holding this
view never holds the scheduler. Reduced motion uses discrete updates and the
agenda provides equivalent keyboard access.

This local perspective renderer needs neither Box free-space physics nor
terrain. Build it after the ordinary Calendar works.

## Simulation previews

Previewing an Action, Karma Rule or Transfer shows its possible changes beside
the current state. A shared scenario selection can drive Records, quantities
and time Sands. Name the scenario, starting revision, assumptions and time;
returning to Live restores the original view state.

Observed Facts, scheduled commitments and simulated results keep different
labels and line treatments. A proposed Transfer completion remains an
assumption. Show changed fields, affected Records and reasons, including
results outside the visible period or filtered out by the proposed change.

This needs a bounded, permission-filtered simulation response for the proposed
operation, not just the existing quantity forecast. Implement missing domain
support with this surface. Unsupported consequences, external effects and
unavailable inputs are named as unknown or unsupported. Never silently skip
them while reporting a complete successful simulation. Previewing must not
write real Facts, send messages or execute external commands.

Applying a proposal uses ordinary Actions, permissions and preconditions.
Recheck changed source revisions and show the new difference before accepting
a stale proposal. Never copy a simulated database wholesale into the live one.
Partial application must explain dependencies and is unavailable where those
dependencies cannot be preserved.

Calendar and Clock ship independently of simulation. Scenario mode is complete
only when one real Action, Rule and Transfer can each be previewed, explained
and compared without external effects, with failures visible.
