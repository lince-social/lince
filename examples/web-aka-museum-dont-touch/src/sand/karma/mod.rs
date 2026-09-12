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
const APP_CANVAS_JS: &str = include_str!("app/canvas.js");
const APP_BLOCKS_JS: &str = include_str!("app/blocks.js");
const APP_BUILDER_JS: &str = include_str!("app/builder.js");
const APP_FREQUENCY_JS: &str = include_str!("app/frequency.js");
const APP_EXECUTION_JS: &str = include_str!("app/execution.js");

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
    assets.insert("app/canvas.js".into(), APP_CANVAS_JS.as_bytes().to_vec());
    assets.insert("app/blocks.js".into(), APP_BLOCKS_JS.as_bytes().to_vec());
    assets.insert("app/builder.js".into(), APP_BUILDER_JS.as_bytes().to_vec());
    assets.insert(
        "app/frequency.js".into(),
        APP_FREQUENCY_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/execution.js".into(),
        APP_EXECUTION_JS.as_bytes().to_vec(),
    );

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
                script src="/board/vendor/d3.v7.min.js" {}
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
        assert!(html.contains("/board/vendor/d3.v7.min.js"));
        assert!(html.contains("app/main.js"));
        for asset in [
            "styles.css",
            "app/state.js",
            "app/format.js",
            "app/entries.js",
            "app/recurrence.js",
            "app/graph.js",
            "app/canvas.js",
            "app/blocks.js",
            "app/builder.js",
            "app/frequency.js",
            "app/execution.js",
        ] {
            assert!(assets.contains(&asset), "missing {asset}");
        }
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
        let html = package().html_document();
        assert!(html.contains("capture-form"), "one-line capture");
        assert!(html.contains("recurrence-form"), "recurring control");
        assert!(html.contains("timeline-graph"), "concept graph");
        assert!(html.contains("occurrence-list"), "what a rule expects next");
    }

    #[test]
    fn every_step_component_is_reachable_down_to_milliseconds() {
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
            assert!(
                html.contains(field),
                "the {field} component must be typeable"
            );
        }
    }

    #[test]
    fn the_main_view_is_a_canvas_behind_a_rules_panel() {
        let html = package().html_document();
        assert!(
            html.contains(r#"id="karma-canvas""#),
            "the card deck's canvas"
        );
        assert!(
            html.contains(r#"id="rules-panel""#),
            "the tucked-away form panel"
        );
        assert!(
            html.contains(r#"id="open-rules-panel""#),
            "the button that opens it"
        );
        assert!(
            html.contains(r#"class="sand-tools""#),
            "the kanban-style hover corner"
        );
        assert!(
            html.contains(r#"class="page-corner""#),
            "the corner triangle itself"
        );
    }

    #[test]
    fn the_panel_leads_with_the_builder_then_frequencies_then_the_slop() {
        let html = package().html_document();
        let builder = html.find("rule-builder-form").expect("the builder");
        let frequency = html.find("frequency-form").expect("frequency CRUD");
        let slop = html.find("slop down here").expect("the honest divider");
        let capture = html.find("capture-form").expect("the older surface");
        assert!(builder < frequency, "the builder comes first");
        assert!(frequency < slop, "frequencies sit above the divider");
        assert!(slop < capture, "everything older sits below it");
    }

    #[test]
    fn a_condition_is_written_with_blocks_and_a_threshold_and_a_consequence() {
        let html = package().html_document();
        assert!(
            html.contains(r#"id="condition-input""#),
            "the block-completing input"
        );
        assert!(
            html.contains(r#"id="condition-suggest""#),
            "the completion list"
        );
        assert!(
            html.contains(r#"id="condition-chips""#),
            "the blocks it names"
        );
        assert!(html.contains(r#"id="builder-gate""#), "the threshold");
        assert!(
            html.contains(r#"id="builder-consequence""#),
            "the consequence"
        );
        assert!(
            html.contains(r#"id="record-search""#),
            "find a record by head or slug"
        );
        assert!(
            html.contains(r#"id="condition-bank""#),
            "conditions other rules read"
        );
        assert!(
            html.contains(r#"id="consequence-bank""#),
            "consequences they run"
        );
    }

    #[test]
    fn a_rule_can_be_changed_after_it_is_declared() {
        let html = package().html_document();
        assert!(
            html.contains("recurrence-submit"),
            "the submit label switches"
        );
        assert!(
            super::APP_RECURRENCE_JS.contains("revise-recurrence"),
            "revising must be reachable"
        );
        assert!(
            super::APP_RECURRENCE_JS.contains("expected_revision"),
            "a revise quotes the revision back, or a stale form wins by being slow"
        );
    }

    #[test]
    fn a_declined_date_is_visible_and_can_be_taken_back() {
        assert!(
            super::APP_RECURRENCE_JS.contains("unskip-recurrence-occurrence"),
            "a skip must be reversible from the surface that made it"
        );
        assert!(
            super::APP_RECURRENCE_JS.contains(r#"o.state === "skipped""#),
            "declined dates must be listed, not filtered away"
        );
    }

    #[test]
    fn a_schedule_is_offered_as_a_term_in_the_arithmetic() {
        let html = package().html_document();
        assert!(
            html.contains("freq(@rule)"),
            "a rhythm must be a readable term"
        );
        assert!(
            html.contains("value(@rule)"),
            "so must another rule's number"
        );
        assert!(
            html.contains("sum_pos"),
            "and the two flow directions, which a net cannot answer"
        );
    }

    #[test]
    fn a_rule_can_be_told_to_look_before_it_acts() {
        let html = package().html_document();
        assert!(html.contains("rule-condition"), "the reading to test");
        assert!(html.contains("rule-gate"), "whether it means fire");
        assert!(html.contains("rule-carry"), "what the consequence receives");
        assert!(
            super::APP_RECURRENCE_JS.contains("const:"),
            "a fixed carry must be expressible"
        );
    }

    #[test]
    fn every_consequence_a_rule_can_carry_is_authorable() {
        let html = package().html_document();
        for kind in [
            "capture-entry",
            "add-quantity",
            "set-quantity",
            "set-quantity-where",
        ] {
            assert!(
                html.contains(kind),
                "the {kind} consequence needs a control"
            );
        }
        assert!(html.contains("rule-concept-action"), "concept consequences");
        for action in [r#"value="add""#, r#"value="remove""#, r#"value="move""#] {
            assert!(html.contains(action), "concept action {action}");
        }
        assert!(
            html.contains(r#"value="none""#),
            "a rule must be able to leave the number alone"
        );
    }

    #[test]
    fn the_weekday_landing_and_short_month_choice_are_offered() {
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
        let html = package().html_document();
        assert!(
            html.contains(r#"id="rule-anchor" type="datetime-local""#),
            "the anchor needs a time of day"
        );
        assert!(html.contains(r#"step="0.001""#), "down to the millisecond");
    }

    #[test]
    fn a_rule_can_be_told_where_to_stop_including_after_one() {
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
        let html = package().html_document();
        assert!(html.contains("occurrence-more"), "room to report a prefix");
        assert!(
            super::APP_STATE_JS.contains("at_since"),
            "the inbox must choose the window it can explain"
        );
        assert!(
            super::APP_RECURRENCE_JS.contains("are not listed"),
            "and say where that window stops"
        );
    }

    #[test]
    fn no_total_or_direction_control_is_offered() {
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

    #[test]
    fn where_rules_run_is_a_per_cell_surface_and_says_so() {
        let html = package().html_document();
        assert!(html.contains("id=\"execution-list\""));
        assert!(html.contains("id=\"execution-empty\""));
        assert!(
            html.contains("Where rules run"),
            "the panel has to be findable by the question it answers"
        );
        assert!(
            html.contains("running on your other Cells"),
            "the panel must state that other Cells are unaffected"
        );
        let lowered = html.to_lowercase();
        for forbidden in ["disable rule", "pause rule", "turn off rule"] {
            assert!(
                !lowered.contains(forbidden),
                "{forbidden:?} describes the rule; this control is about one Cell"
            );
        }
    }

    #[test]
    fn the_execution_panel_has_something_to_say_when_there_is_nothing_to_show() {
        let html = package().html_document();
        assert!(html.contains("No rules on this Cell yet."));
        assert!(
            html.contains("unless you say otherwise"),
            "the default has to be stated, since absence means execute"
        );
    }

    #[test]
    fn turning_a_rule_off_can_carry_a_reason() {
        assert!(super::APP_EXECUTION_JS.contains("data-execution-note"));
        assert!(
            super::APP_EXECUTION_JS.contains("(optional)"),
            "a demanded reason just teaches people to type a space"
        );
    }

    #[test]
    fn an_outward_rule_says_what_running_it_twice_would_do() {
        assert!(
            super::APP_EXECUTION_JS.contains("externally_observable"),
            "the panel must read the outward-consequence flag"
        );
        assert!(
            super::APP_EXECUTION_JS.contains("it acts twice"),
            "the cost has to be stated at the moment of choosing"
        );
    }

    #[test]
    fn an_executor_can_be_designated_and_undesignated_from_the_cell_itself() {
        let js = super::APP_EXECUTION_JS;
        assert!(js.contains("designate-karma-executor"));
        assert!(
            js.contains("Only this Cell") && js.contains("Let any Cell run it"),
            "designation has to be reversible from the same control"
        );
        assert!(
            js.contains("designated to another Cell"),
            "a rule that runs elsewhere must say so rather than looking idle"
        );
    }

    #[test]
    fn the_execution_panel_asks_nothing_through_a_browser_modal() {
        assert!(!super::APP_EXECUTION_JS.contains("confirm("));
        assert!(!super::APP_EXECUTION_JS.contains("prompt("));
        assert!(!super::APP_EXECUTION_JS.contains("alert("));
    }
}
