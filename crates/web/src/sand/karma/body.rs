use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="karmaApp" {
            div class="canvasView" {
                canvas id="karma-canvas" aria-label="Records a rule touches" {}
                p id="canvas-empty" class="empty canvasEmpty" hidden {
                    "No rule touches a record yet."
                }
            }

            p id="notice" class="notice" role="status" hidden {}

            div class="sand-tools" {
                span id="live-dot" class="page-corner" data-live="false" role="status"
                    aria-label="Connection status" title="Connection status" {}
                div class="sand-tools__row" {
                    button id="open-rules-panel" type="button" class="primaryButton" {
                        "+ Rule"
                    }
                }
            }

            aside id="rules-panel" class="sidePanel" hidden {
                div class="sidePanelHead" {
                    h2 { "Rules" }
                    button id="close-rules-panel" type="button" class="ghostButton" {
                        "Close"
                    }
                }
                div class="sidePanelBody" {

            section class="panel builderPanel" aria-labelledby="builder-heading" {
                h2 id="builder-heading" { "New rule" }

                form id="rule-builder-form" class="stackForm" {
                    div class="builderStep" {
                        div class="builderStepHead" {
                            h3 { "Condition" }
                            button id="condition-bank-toggle" type="button"
                                class="ghostButton" { "Reuse…" }
                        }
                        div class="smartInputWrap" {
                            input id="condition-input" type="text" class="smartInput"
                                role="combobox" aria-autocomplete="list"
                                aria-expanded="false" aria-controls="condition-suggest"
                                autocomplete="off" spellcheck="false"
                                placeholder="-1 * freq(@daily) + @apple";
                            ul id="condition-suggest" class="suggestList"
                                role="listbox" aria-label="Blocks" hidden {}
                        }
                        div id="condition-chips" class="chipStrip" {}
                        p id="condition-error" class="fieldError" role="alert" hidden {}
                        p class="hint" {
                            "Type " code { "@" } " for any block, or "
                            code { "record(" } " / " code { "freq(" } " to narrow it. "
                            code { "Tab" } " takes the first match."
                        }
                        ul id="condition-bank" class="bankList" hidden {}
                        p id="condition-bank-empty" class="empty" hidden {
                            "No other rule has a condition to borrow yet."
                        }
                    }

                    div class="builderStep" {
                        h3 { "Threshold" }
                        label class="field" {
                            span { "Passes when the reading" }
                            select id="builder-gate" {
                                option value="!=0" selected { "is not zero" }
                                option value="always" { "is anything" }
                                option value="<" { "is less than…" }
                                option value="<=" { "is at most…" }
                                option value=">" { "is more than…" }
                                option value=">=" { "is at least…" }
                                option value="==" { "equals…" }
                            }
                        }
                        label class="field" id="builder-gate-value-field" hidden {
                            span { "That number" }
                            input id="builder-gate-value" type="text"
                                inputmode="decimal" placeholder="3" autocomplete="off";
                        }
                    }

                    div class="builderStep" {
                        div class="builderStepHead" {
                            h3 { "Consequence" }
                            button id="consequence-bank-toggle" type="button"
                                class="ghostButton" { "Reuse…" }
                        }
                        label class="field" {
                            span { "To" }
                            select id="builder-target" required {}
                        }
                        label class="field" {
                            span { "Do" }
                            select id="builder-consequence" {
                                option value="capture-entry" selected { "Capture an amount" }
                                option value="add-quantity" { "Add to the quantity" }
                                option value="set-quantity" { "Set the quantity to" }
                                option value="set-quantity-where" { "Set the quantity of everything with a concept" }
                                option value="add-concept" { "Add a concept" }
                                option value="remove-concept" { "Remove a concept" }
                                option value="run-command" { "Run a shell command" }
                            }
                        }
                        label class="field" id="builder-amount-field" {
                            span { "Amount" }
                            input id="builder-amount" type="text" inputmode="decimal"
                                placeholder="what the condition carried" autocomplete="off";
                        }
                        label class="field" id="builder-concept-field" hidden {
                            span { "Concept" }
                            input id="builder-concept" type="text" list="concept-options"
                                placeholder="@done" autocomplete="off";
                        }
                        label class="field" id="builder-command-field" hidden {
                            span { "Command" }
                            input id="builder-command" type="text"
                                placeholder="notify-send 'rent due'" autocomplete="off";
                        }
                        ul id="consequence-bank" class="bankList" hidden {}
                        p id="consequence-bank-empty" class="empty" hidden {
                            "No other rule has a consequence to borrow yet."
                        }
                    }

                    div class="builderStep" {
                        h3 { "Records" }
                        label class="field" {
                            span class="visuallyHidden" { "Find a record" }
                            input id="record-search" type="search"
                                placeholder="find by head or slug" autocomplete="off";
                        }
                        ul id="record-results" class="recordResults" {}
                        p id="record-results-empty" class="empty" hidden {
                            "Nothing here by that name."
                        }
                    }

                    div class="formActions" {
                        button id="builder-submit" type="submit" class="primaryButton" {
                            "Create rule"
                        }
                        button id="builder-reset" type="button" class="ghostButton" {
                            "Clear"
                        }
                    }
                }
            }

            section class="panel frequencyPanel" aria-labelledby="frequency-heading" {
                div class="panelHead" {
                    h2 id="frequency-heading" { "Frequencies" }
                    button id="toggle-frequency-form" type="button" class="ghostButton" {
                        "+ New frequency"
                    }
                }

                form id="frequency-form" class="stackForm" hidden {
                    label class="field" {
                        span { "Called" }
                        input id="frequency-slug" type="text" placeholder="daily"
                            autocomplete="off" required;
                    }
                    label class="field" {
                        span { "Preset" }
                        select id="frequency-preset" {
                            option value="daily" selected { "Every day" }
                            option value="weekly" { "Every week" }
                            option value="fortnightly" { "Every two weeks" }
                            option value="monthly" { "Every month" }
                            option value="yearly" { "Every year" }
                            option value="custom" { "Custom…" }
                        }
                    }
                    fieldset class="stepGrid" {
                        legend { "Every" }
                        label class="stepUnit" {
                            span { "Years" }
                            input id="freq-years" type="number" min="0" value="0"
                                aria-label="Years between beats";
                        }
                        label class="stepUnit" {
                            span { "Months" }
                            input id="freq-months" type="number" min="0" value="0"
                                aria-label="Months between beats";
                        }
                        label class="stepUnit" {
                            span { "Weeks" }
                            input id="freq-weeks" type="number" min="0" value="0"
                                aria-label="Weeks between beats";
                        }
                        label class="stepUnit" {
                            span { "Days" }
                            input id="freq-days" type="number" min="0" value="1"
                                aria-label="Days between beats";
                        }
                        label class="stepUnit" {
                            span { "Hours" }
                            input id="freq-hours" type="number" min="0" value="0"
                                aria-label="Hours between beats";
                        }
                        label class="stepUnit" {
                            span { "Minutes" }
                            input id="freq-minutes" type="number" min="0" value="0"
                                aria-label="Minutes between beats";
                        }
                        label class="stepUnit" {
                            span { "Seconds" }
                            input id="freq-seconds" type="number" min="0" value="0"
                                aria-label="Seconds between beats";
                        }
                        label class="stepUnit" {
                            span { "Millis" }
                            input id="freq-milliseconds" type="number" min="0" value="0"
                                aria-label="Milliseconds between beats";
                        }
                    }
                    label class="field" {
                        span { "Starting" }
                        input id="frequency-anchor" type="datetime-local" step="0.001";
                    }
                    div class="formActions" {
                        button id="frequency-submit" type="submit" class="primaryButton" {
                            "Create frequency"
                        }
                        button id="cancel-frequency" type="button" class="ghostButton" {
                            "Cancel"
                        }
                    }
                    p id="frequency-preview" class="hint" aria-live="polite" {}
                }

                ul id="frequency-list" class="ruleList" {}
                p id="frequency-empty" class="empty" hidden {
                    "No frequencies yet."
                }
            }

            div class="slopDivider" role="separator" { span { "slop down here" } }

            section class="panel capturePanel" aria-labelledby="capture-heading" {
                h2 id="capture-heading" { "Capture" }
                form id="capture-form" class="lineForm" {
                    label class="field" {
                        span { "Resource" }
                        select id="capture-record" required {}
                    }
                    label class="field amountField" {
                        span { "Amount" }
                        input id="capture-amount" type="text" inputmode="decimal"
                            placeholder="-10.50" autocomplete="off" required;
                    }
                    label class="field" {
                        span { "For" }
                        input id="capture-concept" type="text" list="concept-options"
                            placeholder="@food" autocomplete="off";
                    }
                    label class="field" {
                        span { "When" }
                        input id="capture-at" type="date";
                    }
                    label class="field noteField" {
                        span { "Note" }
                        input id="capture-note" type="text" placeholder="ice cream" autocomplete="off";
                    }
                    button type="submit" class="primaryButton" { "Capture" }
                }
                p class="hint" {
                    "A negative amount is a cost, a positive one a gain. A refund is the same "
                    "category with a positive amount — there is no direction to choose."
                }
                datalist id="concept-options" {}
            }

            div class="columns" {
                section class="panel" aria-labelledby="entries-heading" {
                    div class="panelHead" {
                        h2 id="entries-heading" { "Changes" }
                        label class="field inlineField" {
                            span class="visuallyHidden" { "Filter changes by category" }
                            input id="entries-filter" type="text" list="concept-options"
                                placeholder="all categories" autocomplete="off";
                        }
                    }
                    ul id="entry-list" class="entryList" {}
                    p id="entries-empty" class="empty" hidden { "Nothing captured yet." }
                }

                section class="panel" aria-labelledby="recurrence-heading" {
                    div class="panelHead" {
                        h2 id="recurrence-heading" { "Recurring" }
                        button id="toggle-recurrence-form" type="button" class="ghostButton" {
                            "+ New rule"
                        }
                    }

                    form id="recurrence-form" class="stackForm" hidden {
                        label class="field" {
                            span { "Resource" }
                            select id="rule-record" required {}
                        }
                        fieldset class="thenGroup" {
                            legend { "Only if" }
                            label class="field" {
                                span { "This reading" }
                                input id="rule-condition" type="text" autocomplete="off"
                                    placeholder="-1 * freq(@payday)";
                            }
                            p class="hint" id="rule-condition-hint" {
                                "Readings: "
                                code { "@record" } ", "
                                code { "freq(@rule)" } ", "
                                code { "value(@rule)" } ", "
                                code { "sum(@record, 30d)" } " — "
                                code { "sum_pos" } "/" code { "sum_neg" } " split the directions."
                            }
                            label class="field" id="rule-gate-field" hidden {
                                span { "Passes" }
                                select id="rule-gate" {
                                    option value="!=0" selected { "is not zero" }
                                    option value="always" { "always (any value)" }
                                    option value="<" { "is less than…" }
                                    option value="<=" { "is at most…" }
                                    option value=">" { "is more than…" }
                                    option value=">=" { "is at least…" }
                                    option value="==" { "equals…" }
                                }
                            }
                            label class="field" id="rule-gate-value-field" hidden {
                                span { "That number" }
                                input id="rule-gate-value" type="text" inputmode="decimal"
                                    placeholder="3" autocomplete="off";
                            }
                            label class="field" id="rule-carry-field" hidden {
                                span { "And the amount is" }
                                select id="rule-carry" {
                                    option value="value" selected { "the reading itself" }
                                    option value="one" { "one" }
                                    option value="const" { "a fixed number…" }
                                }
                            }
                            label class="field" id="rule-carry-value-field" hidden {
                                span { "That fixed number" }
                                input id="rule-carry-value" type="text" inputmode="decimal"
                                    placeholder="-1" autocomplete="off";
                            }
                            p class="hint" {
                                "Leave the reading empty for a rule the date alone justifies. "
                                "A reading is an expression over records: "
                                "@slug, sum(@slug, 30d), arithmetic and comparisons."
                            }
                        }

                        fieldset class="thenGroup" {
                            legend { "Then" }
                            label class="field" {
                                span { "To the number" }
                                select id="rule-number-action" {
                                    option value="capture-entry" selected { "Capture an amount" }
                                    option value="add-quantity" { "Add to the quantity" }
                                    option value="set-quantity" { "Set the quantity to" }
                                    option value="set-quantity-where" { "Set the quantity of everything with a concept" }
                                    option value="none" { "Nothing" }
                                }
                            }
                            label class="field" id="rule-amount-field" {
                                span { "Amount" }
                                input id="rule-amount" type="text" inputmode="decimal"
                                    placeholder="-1200" autocomplete="off";
                            }
                            label class="field" id="rule-concept-field" {
                                span { "For" }
                                input id="rule-concept" type="text" list="concept-options"
                                    placeholder="@rent" autocomplete="off";
                            }

                            label class="field" {
                                span { "To the concepts" }
                                select id="rule-concept-action" {
                                    option value="none" selected { "Nothing" }
                                    option value="add" { "Add one" }
                                    option value="remove" { "Remove one" }
                                    option value="move" { "Move from one to another" }
                                    option value="set" { "Replace all with one" }
                                }
                            }
                            label class="field" id="rule-concept-from-field" hidden {
                                span { "From" }
                                input id="rule-concept-from" type="text" list="concept-options"
                                    placeholder="@wip" autocomplete="off";
                            }
                            label class="field" id="rule-concept-to-field" hidden {
                                span { "Concept" }
                                input id="rule-concept-to" type="text" list="concept-options"
                                    placeholder="@done" autocomplete="off";
                            }
                        }
                        label class="field" {
                            span { "Preset" }
                            select id="rule-preset" {
                                option value="monthly" selected { "Monthly" }
                                option value="weekly" { "Weekly" }
                                option value="fortnightly" { "Fortnightly" }
                                option value="daily" { "Daily" }
                                option value="yearly" { "Yearly" }
                                option value="custom" { "Custom…" }
                            }
                        }

                        fieldset class="stepGrid" {
                            legend { "Repeats every" }
                            label class="stepUnit" {
                                span { "Years" }
                                input id="step-years" type="number" min="0" value="0"
                                    aria-label="Years between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Months" }
                                input id="step-months" type="number" min="0" value="1"
                                    aria-label="Months between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Weeks" }
                                input id="step-weeks" type="number" min="0" value="0"
                                    aria-label="Weeks between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Days" }
                                input id="step-days" type="number" min="0" value="0"
                                    aria-label="Days between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Hours" }
                                input id="step-hours" type="number" min="0" value="0"
                                    aria-label="Hours between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Minutes" }
                                input id="step-minutes" type="number" min="0" value="0"
                                    aria-label="Minutes between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Seconds" }
                                input id="step-seconds" type="number" min="0" value="0"
                                    aria-label="Seconds between occurrences";
                            }
                            label class="stepUnit" {
                                span { "Millis" }
                                input id="step-milliseconds" type="number" min="0" value="0"
                                    aria-label="Milliseconds between occurrences";
                            }
                        }

                        fieldset class="weekdayGrid" {
                            legend { "Then move forward to" }
                            @for (value, label) in [
                                ("monday", "Mon"), ("tuesday", "Tue"), ("wednesday", "Wed"),
                                ("thursday", "Thu"), ("friday", "Fri"), ("saturday", "Sat"),
                                ("sunday", "Sun"),
                            ] {
                                label class="weekdayChoice" {
                                    input type="checkbox" name="land-on" value=(value);
                                    span { (label) }
                                }
                            }
                            p class="hint" {
                                "Leave all clear to keep the date the step lands on."
                            }
                        }

                        label class="field" id="rule-invalid-day-field" {
                            span { "When a month is too short" }
                            select id="rule-invalid-day" {
                                option value="clamp" selected { "Use the last day of that month" }
                                option value="skip" { "Skip that month" }
                            }
                        }

                        label class="field" {
                            span { "Starting" }
                            input id="rule-anchor" type="datetime-local" step="0.001";
                        }

                        label class="field" id="rule-bound-field" {
                            span { "Repeating" }
                            select id="rule-bound" {
                                option value="unbounded" selected { "Until I stop it" }
                                option value="count" { "A set number of times" }
                                option value="until" { "Until a date" }
                            }
                        }
                        label class="field" id="rule-bound-count-field" hidden {
                            span { "How many times" }
                            input id="rule-bound-count" type="number" min="1" step="1" value="1";
                        }
                        label class="field" id="rule-bound-until-field" hidden {
                            span { "Stopping before" }
                            input id="rule-bound-until" type="datetime-local" step="0.001";
                        }
                        label class="field" {
                            span { "Note" }
                            input id="rule-note" type="text" placeholder="rent" autocomplete="off";
                        }
                        div class="formActions" {
                            button id="recurrence-submit" type="submit" class="primaryButton" {
                                "Declare rule"
                            }
                            button type="button" id="cancel-recurrence" class="ghostButton" { "Cancel" }
                        }
                        p class="hint" {
                            "A rule declares what is expected. It writes nothing until a date is applied. "
                            "The starting date sets the day of the month and the time of day every "
                            "occurrence inherits."
                        }
                        p id="rule-preview" class="hint" aria-live="polite" {}
                    }

                    ul id="recurrence-list" class="ruleList" {}
                    p id="recurrence-empty" class="empty" hidden { "No recurring rules yet." }

                    h3 class="subHeading" { "Expected next" }
                    ul id="occurrence-list" class="occurrenceList" {}
                    p id="occurrence-empty" class="empty" hidden { "Nothing expected in this window." }
                    p id="occurrence-more" class="hint" aria-live="polite" hidden {}
                }
            }

            section class="panel executionPanel" aria-labelledby="execution-heading" {
                div class="panelHead" {
                    h2 id="execution-heading" { "Where rules run" }
                }
                p class="hint" {
                    "Every rule you hold runs on this Cell unless you say otherwise. "
                    "Turning one off here leaves it running on your other Cells — "
                    "it is this device's setting, not a change to the rule."
                }
                ul id="execution-list" class="ruleList" {}
                p id="execution-empty" class="empty" hidden {
                    "No rules on this Cell yet."
                }
                p id="execution-notice" class="hint" role="status" aria-live="polite" hidden {}
            }

            section class="panel graphPanel" aria-labelledby="graph-heading" {
                div class="panelHead" {
                    h2 id="graph-heading" { "Concept over time" }
                    div class="graphControls" {
                        label class="field inlineField" {
                            span class="visuallyHidden" { "Concept to chart" }
                            input id="graph-concept" type="text" list="concept-options"
                                placeholder="@cost" autocomplete="off";
                        }
                        select id="graph-window" aria-label="Time window" {
                            option value="6" selected { "±6 months" }
                            option value="12" { "±12 months" }
                            option value="24" { "±24 months" }
                        }
                    }
                }

                div class="currentState" {
                    div class="stateBlock" {
                        span class="stateLabel" { "Now" }
                        strong id="state-current" class="stateValue" { "—" }
                    }
                    div class="stateBlock" {
                        span class="stateLabel" { "Before window" }
                        span id="state-opening" class="stateValueSmall" { "—" }
                    }
                    div class="stateBlock" {
                        span class="stateLabel" { "Declared ahead" }
                        span id="state-expected" class="stateValueSmall" { "—" }
                    }
                }

                div id="timeline-graph" class="graph" role="img"
                    aria-describedby="timeline-table-caption" {}

                details class="tableDetails" open {
                    summary id="timeline-table-caption" { "Points behind this chart" }
                    div class="tableScroll" {
                        table id="timeline-table" class="dataTable" {
                            thead {
                                tr {
                                    th scope="col" { "Period" }
                                    th scope="col" { "Settled" }
                                    th scope="col" { "Declared" }
                                    th scope="col" { "Running" }
                                }
                            }
                            tbody {}
                        }
                    }
                }

                h3 class="subHeading" { "What the future is made of" }
                ul id="contributor-list" class="contributorList" {}
                p id="contributor-empty" class="empty" hidden {
                    "Nothing is declared ahead for this concept."
                }
            }
                }
            }
        }
    }
}
