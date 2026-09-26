pub const COUNT: usize = 38;

pub fn source(index: usize) -> &'static str {
    match index {
        0 => minus(),
        1 => plus(),
        2 => reset(),
        3 => recenter(),
        4 => bring_here(),
        5 => paintbrush(),
        6 => close(),
        7 => check(),
        8 => workspaces(),
        9 => palette(),
        10 => store(),
        11 => save(),
        12 => pencil(),
        13 => text(),
        14 => editable_text(),
        15 => scroll(),
        16 => grow(),
        17 => circle(),
        18 => square(),
        19 => info(),
        20 => bell(),
        21 => pin(),
        22 => forward(),
        23 => backward(),
        24 => back(),
        25 => delete(),
        26 => general(),
        27 => group(),
        28 => ungroup(),
        29 => attract(),
        30 => repel(),
        31 => play(),
        32 => stop(),
        33 => previous(),
        34 => next(),
        35 => person(),
        36 => engine(),
        37 => credits(),
        _ => panic!("unknown icon index {index}"),
    }
}

fn minus() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M5 12h14"/><circle cx="4" cy="12" r="0.6" fill="currentColor" stroke="none"/><circle cx="20" cy="12" r="0.6" fill="currentColor" stroke="none"/></svg>"##
}

fn plus() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M12 4.5v15M4.5 12h15"/><circle cx="12" cy="12" r="1.6" fill="currentColor" stroke="none"/></svg>"##
}

fn reset() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M6 8.5a7.5 7.5 0 1 1-1.1 6.4"/><path d="M4.5 5.5v4.5H9"/><circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none"/></svg>"##
}

fn recenter() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 9V5h4M16 5h4v4M20 15v4h-4M8 19H4v-4"/><circle cx="12" cy="12" r="3.5"/><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none"/></svg>"##
}

fn bring_here() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"><path d="M4 4h10v10H4z"/><path d="M10 10h10v10H10z"/><path d="M14 5h5v5"/><path d="m15.5 8.5 3.5-3.5"/></svg>"##
}

fn paintbrush() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m14 10 5.8-5.8a1.5 1.5 0 0 1 2 2L16 12"/><path d="M6 13c2-2 6-2 8 0l1 1c-1 3-4 5-7 5-2 0-4 1-5 2 1-4 0-5 3-8z"/><path d="M8 16h.1"/></svg>"##
}

fn close() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 6 12 12M18 6 6 18"/><circle cx="12" cy="12" r="9" stroke-width="1" stroke-dasharray="1 3"/></svg>"##
}

fn check() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m4.5 12.5 5 5 10-11"/><path d="M5 7a9 9 0 0 1 10-2" stroke-width="1.4"/></svg>"##
}

fn workspaces() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M4 5h11v10H4zM9 10h11v10H9z"/><path d="M6 7h2M11 12h2" stroke-linecap="round"/></svg>"##
}

fn palette() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 3a9 9 0 1 0 0 18h1c2 0 2-2 1-3s-1-3 1-3h3c3 0 4-3 2-6-2-4-5-6-8-6z"/><g fill="currentColor" stroke="none"><circle cx="7" cy="10" r="1.3"/><circle cx="11" cy="7" r="1.3"/><circle cx="16" cy="8" r="1.3"/><circle cx="6.5" cy="15" r="1.3"/></g></svg>"##
}

fn store() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"><path d="M5 10V5h14v5M5 12v8h14v-8"/><path d="M3 10h18v2l-2 2-3-2-4 2-4-2-3 2-2-2z"/><path d="M9 20v-5h6v5"/></svg>"##
}

fn save() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 4h12l3 3v13H4V4z"/><path d="M8 4v6h8V4M8 20v-6h8v6"/><circle cx="17" cy="8" r=".8" fill="currentColor" stroke="none"/></svg>"##
}

fn pencil() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="m4 20 5-.8L20 8l-4-4L5 15zM13 7l4 4M5 15l4 4"/><path d="M3 22h18" stroke-linecap="round"/></svg>"##
}

fn text() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M4 7V5h16v2M12 5v14M8 19h8"/><path d="M5 10h4M15 10h4" stroke-width="1.3"/></svg>"##
}

fn editable_text() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M3 6h12M9 6v12M6 18h6M18 5v14M16 5h4M16 19h4"/><path d="M3 22h18" stroke-width="1"/></svg>"##
}

fn scroll() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 6h10M5 10h9M5 14h8M5 18h10M19 6v12m-2-9 2-3 2 3m-4 6 2 3 2-3"/></svg>"##
}

fn grow() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 17V7h10M5 12h8M5 17h11M18 16v5m-3-3 3 3 3-3M18 8V3m-3 3 3-3 3 3"/></svg>"##
}

fn circle() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"><circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none"/></svg>"##
}

fn square() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M4 4h16v16H4z"/><path d="M7 7h2M15 17h2" stroke-linecap="round" stroke-width="1.2"/></svg>"##
}

fn info() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 3 20 7v10l-8 4-8-4V7z" stroke-linejoin="round"/><path d="M12 11v6" stroke-linecap="round"/><circle cx="12" cy="7.8" r="1" fill="currentColor" stroke="none"/></svg>"##
}

fn bell() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 17h14l-2-3V9a5 5 0 0 0-10 0v5zM9.5 20h5"/><path d="M12 3V2"/></svg>"##
}

fn pin() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3h8M9 3v7l-3 5h12l-3-5V3M12 15v6"/></svg>
"##
}

fn forward() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5m-5 5 5-5 5 5M5 20h14"/></svg>
"##
}

fn backward() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14m-5-5 5 5 5-5M5 4h14"/></svg>
"##
}

fn back() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h10v4H8v6H4zM10 10h10v10H10z"/></svg>
"##
}

fn delete() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7"/></svg>
"##
}

fn general() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h4m6 0h8M3 12h10m6 0h2M3 18h2m6 0h10"/><circle cx="10" cy="6" r="3"/><circle cx="16" cy="12" r="3"/><circle cx="8" cy="18" r="3"/></svg>
"##
}

fn group() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 3H3v4m14-4h4v4M3 17v4h4m14-4v4h-4"/><rect x="7" y="7" width="6" height="6"/><path d="M13 11h4v6h-6v-4"/></svg>
"##
}

fn ungroup() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><path d="M15 3h6v6m-6 0 6-6M3 15v6h6m-6 0 6-6"/></svg>
"##
}

fn attract() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h6m-3-3 3 3-3 3M22 12h-6m3-3-3 3 3 3M12 2v6m-3-3 3 3 3-3M12 22v-6m-3 3 3-3 3 3"/></svg>
"##
}

fn repel() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 12H2m3-3-3 3 3 3M16 12h6m-3-3 3 3-3 3M12 8V2M9 5l3-3 3 3M12 16v6m-3-3 3 3 3-3"/></svg>
"##
}

fn play() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m8 4 12 8-12 8z"/></svg>
"##
}

fn stop() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="1"/></svg>
"##
}

fn previous() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 5-7 7 7 7"/></svg>
"##
}

fn next() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 5 7 7-7 7"/></svg>
"##
}

fn person() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="7" r="4"/><path d="M4 21v-2a8 8 0 0 1 16 0v2"/></svg>
"##
}

fn engine() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 3 1-1h4l1 3 2 1 3-1 2 4-2 2v2l2 2-2 4-3-1-2 1-1 3h-4l-1-3-2-1-3 1-2-4 2-2v-2L2 9l2-4 3 1 2-1z"/><circle cx="12" cy="12" r="3"/></svg>
"##
}

fn credits() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6"/><circle cx="12" cy="15" r="4"/><path d="M13 13.5a2 2 0 1 0 0 3"/></svg>
"##
}
