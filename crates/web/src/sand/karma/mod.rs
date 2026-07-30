//! The Karma sand: the human surface over rules.
//!
//! A rule is a trigger, a condition, an arithmetic, and a consequence. This sand
//! is where a person creates, reads, revises and retires them, and where the
//! record they act on is drawn with its past, its present and its declared
//! future on one line.
//!
//! Economy is not a sand. It is what you get when the rules on this surface are
//! about a balance, exactly as a pantry is what you get when they are about
//! flour. The backend cannot tell the difference and must not be able to: the
//! vocabulary — resource, cost, category — is supplied here, in HTML, over
//! primitives that would answer the same questions about hours without a line
//! changing.
//!
//! What it owns: capturing one change, correcting one, declaring a recurring
//! one, answering the instants a schedule produces, and drawing one classified
//! concept through time. What it deliberately does not own: any total, any
//! balance, any projection. Every number on screen arrives already computed from
//! Protein, because the moment JavaScript adds two amounts, exactness is gone.
//!
//! The honest limit today: a declared rule does not fire itself. Authorized
//! intents are durable but inert until the effect worker exists, so applying an
//! occurrence is a human pressing apply. The surface is built so that when the
//! worker lands, nothing here has to change shape — only the consequence a rule
//! is allowed to carry.

mod body;

use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    maud::{DOCTYPE, Markup, html},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.karma";

const STYLES_CSS: &str = include_str!("styles.css");
const APP_MAIN_JS: &str = include_str!("app/main.js");
const APP_STATE_JS: &str = include_str!("app/state.js");
const APP_FORMAT_JS: &str = include_str!("app/format.js");
const APP_ENTRIES_JS: &str = include_str!("app/entries.js");
const APP_RECURRENCE_JS: &str = include_str!("app/recurrence.js");
const APP_GRAPH_JS: &str = include_str!("app/graph.js");

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "◫".into(),
        title: "Karma".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description:
            "Create, revise and retire the rules that change records — one-off, recurring, or conditional — and read any concept through its past, present and declared future."
                .into(),
        details:
            "The rules plane. One-line capture classifies a change without asking which total it belongs to; a schedule declares what is expected — from a single dated promise up to a compound step landing on a chosen weekday — without writing anything; and a concept timeline draws settled history, the current position, and declared future in one line. Economy, a pantry and a training log are the same surface with different records in it. Every total, bucket and graph point is computed by Protein — the client only formats."
                .into(),
        initial_width: 9,
        initial_height: 7,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    };

    let mut assets = BTreeMap::new();
    assets.insert("styles.css".into(), STYLES_CSS.as_bytes().to_vec());
    assets.insert("app/main.js".into(), APP_MAIN_JS.as_bytes().to_vec());
    assets.insert("app/state.js".into(), APP_STATE_JS.as_bytes().to_vec());
    assets.insert("app/format.js".into(), APP_FORMAT_JS.as_bytes().to_vec());
    assets.insert("app/entries.js".into(), APP_ENTRIES_JS.as_bytes().to_vec());
    assets.insert(
        "app/recurrence.js".into(),
        APP_RECURRENCE_JS.as_bytes().to_vec(),
    );
    assets.insert("app/graph.js".into(), APP_GRAPH_JS.as_bytes().to_vec());

    LincePackage::new_archive(
        Some("karma.lince".into()),
        manifest.clone(),
        document(&manifest),
        "index.html",
        assets,
    )
    .expect("karma sand should render as a valid archive package")
}

fn document(manifest: &PackageManifest) -> String {
    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (manifest.title.as_str()) }
                link rel="stylesheet" href="styles.css";
                script src="/board/frame.js" {}
            }
            body {
                (body::body())
                script type="module" src="app/main.js" {}
            }
        }
    };
    markup.into_string()
}

#[cfg(test)]
mod tests {
    use super::package;

    #[test]
    fn package_is_a_live_karma_surface() {
        let package = package();
        let html = package.html_document();
        let assets = package.asset_paths().collect::<Vec<_>>();

        assert!(html.contains("/board/frame.js"));
        assert!(html.contains("app/main.js"));
        for asset in [
            "styles.css",
            "app/state.js",
            "app/format.js",
            "app/entries.js",
            "app/recurrence.js",
            "app/graph.js",
        ] {
            assert!(assets.contains(&asset), "missing {asset}");
        }
        // The builder normalises permission order, so compare as a set.
        let mut permissions = package.manifest.permissions.clone();
        permissions.sort();
        assert_eq!(
            permissions,
            vec![
                "act".to_string(),
                "bridge_state".to_string(),
                "protein_subscribe".to_string(),
            ]
        );
    }

    #[test]
    fn the_capture_line_and_both_control_surfaces_are_present() {
        // The three things the sand exists to do. A refactor that drops one
        // should fail here rather than in someone's hands.
        let html = package().html_document();
        assert!(html.contains("capture-form"), "one-line capture");
        assert!(html.contains("recurrence-form"), "recurring control");
        assert!(html.contains("timeline-graph"), "concept graph");
        assert!(html.contains("occurrence-list"), "what a rule expects next");
    }

    #[test]
    fn every_step_component_is_reachable_down_to_milliseconds() {
        // A backend that accepts `1 month + 1 day + 1 second + 10ms` behind a
        // form offering four presets is not the feature. The whole ladder has
        // to be typeable, or the fine control exists only in tests.
        let html = package().html_document();
        for field in [
            "step-years",
            "step-months",
            "step-weeks",
            "step-days",
            "step-hours",
            "step-minutes",
            "step-seconds",
            "step-milliseconds",
        ] {
            assert!(html.contains(field), "the {field} component must be typeable");
        }
    }

    #[test]
    fn the_weekday_landing_and_short_month_choice_are_offered() {
        // The two adjustments that turn a step into a rule a person actually
        // means: "then move to a Friday", and what a 31st means in February.
        let html = package().html_document();
        assert!(html.contains(r#"name="land-on""#), "weekday landing");
        for day in ["monday", "friday", "sunday"] {
            assert!(html.contains(day), "{day} must be selectable");
        }
        assert!(html.contains("rule-invalid-day"), "short-month policy");
        assert!(
            html.contains("Skip that month"),
            "skipping is the alternative to clamping and must be sayable"
        );
    }

    #[test]
    fn the_anchor_carries_a_time_not_only_a_date() {
        // A rule stepping in seconds is phased by the instant it started at. A
        // date-only anchor would silently round every such rule to midnight.
        let html = package().html_document();
        assert!(
            html.contains(r#"id="rule-anchor" type="datetime-local""#),
            "the anchor needs a time of day"
        );
        assert!(html.contains(r#"step="0.001""#), "down to the millisecond");
    }

    #[test]
    fn a_rule_can_be_told_where_to_stop_including_after_one() {
        // Where a rule ends is part of the rule, and "once, on that day" is the
        // bound set to one. If this control were missing, a one-off promise
        // would need a second kind of object again — which is the split the
        // backend just stopped having.
        let html = package().html_document();
        assert!(html.contains("rule-bound"), "a bound must be choosable");
        assert!(html.contains("rule-bound-count"), "a count of occurrences");
        assert!(html.contains("rule-bound-until"), "a closing date");
        assert!(
            html.contains("Until I stop it"),
            "an unbounded rule must be sayable, and be the default"
        );
    }

    #[test]
    fn a_truncated_list_has_somewhere_to_say_so() {
        // A millisecond rule produces more dates than any page can hold, and a
        // list that stops without saying so reads as an obligation fully met.
        let html = package().html_document();
        assert!(html.contains("occurrence-more"), "room to report a prefix");
    }

    #[test]
    fn no_total_or_direction_control_is_offered() {
        // Two design rules the backend depends on: direction is the amount's
        // sign, and no form may ask which total a change belongs to.
        let html = package().html_document();
        let lowered = html.to_lowercase();
        assert!(
            !lowered.contains(">expense</option>") && !lowered.contains(">income</option>"),
            "direction is the sign of the amount, never a control"
        );
        assert!(
            !lowered.contains("add to total") && !lowered.contains("category total"),
            "totals are queries over classification, never a thing to file into"
        );
    }
}
