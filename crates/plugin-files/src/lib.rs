use nucleus::plugins::{
    Contribution, DocumentFormat, ExportDocument, ExportSnapshot, Field, FieldType, Manifest,
    SnapshotSource,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn manifest(executable: String) -> Manifest {
    let field = |id: &str, name: &str, default: &str| Field {
        id: id.into(),
        name: name.into(),
        kind: FieldType::Text,
        required: true,
        default: json!(default),
    };
    Manifest {
        format: nucleus::plugins::FORMAT.into(),
        id: "lince.files".into(),
        name: "Calendar and document exports".into(),
        version: 1,
        executable,
        arguments: Vec::new(),
        contributions: [
            (
                "ics",
                "iCalendar (.ics)",
                SnapshotSource::Schedule,
                "ics",
                "schedule.ics",
            ),
            (
                "markdown",
                "Markdown (.md)",
                SnapshotSource::Record,
                "md",
                "records.md",
            ),
        ]
        .into_iter()
        .map(
            |(id, name, source, extension, filename)| Contribution::Sync {
                id: id.into(),
                name: name.into(),
                incoming: false,
                outgoing: true,
                replay_safe: true,
                schema: String::new(),
                document: Some(DocumentFormat {
                    source,
                    extension: extension.into(),
                }),
                settings: vec![
                    field("directory", "Destination directory (absolute path)", ""),
                    field("filename", "Filename", filename),
                    field("title", "Calendar or document name", "Lince"),
                ],
            },
        )
        .collect(),
        licenses: vec![nucleus::sand_package::License {
            name: "Lince — MIT".into(),
            text: include_str!("../../../LICENSE").into(),
        }],
        credits: vec!["Lince. iCalendar format follows IETF RFC 5545.".into()],
    }
}

pub fn handle(contribution: &str, operation: &str, input: Value) -> Result<Value, String> {
    if operation == "initialize" {
        return Ok(json!({"protocol":nucleus::plugins::PROTOCOL}));
    }
    if !matches!(contribution, "ics" | "markdown") {
        return Err("Unknown export format".into());
    }
    let settings = &input["settings"];
    let title = settings["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty() && title.len() <= 1024)
        .ok_or("Choose a calendar or document name")?;
    if operation == "validate" {
        return Ok(json!({"valid":true}));
    }
    if operation != "render" {
        return Err("Unknown export operation".into());
    }
    let snapshot: ExportSnapshot =
        serde_json::from_value(input["snapshot"].clone()).map_err(|error| error.to_string())?;
    if snapshot.scope.len() > 256 || snapshot.rows.len() > nucleus::plugins::MAX_SNAPSHOT_ITEMS {
        return Err("Export exceeds its entry budget".into());
    }
    let content = if contribution == "ics" {
        calendar(title, &snapshot)?
    } else {
        markdown(title, &snapshot)?
    };
    if content.len() > nucleus::plugins::MAX_BYTES / 2 {
        return Err("Export exceeds its document budget".into());
    }
    serde_json::to_value(ExportDocument { content }).map_err(|error| error.to_string())
}

fn text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn line(output: &mut String, value: &str) {
    let mut length = 0;
    for character in value.chars() {
        if length + character.len_utf8() > 75 {
            output.push_str("\r\n ");
            length = 1;
        }
        output.push(character);
        length += character.len_utf8();
    }
    output.push_str("\r\n");
}

fn timestamp(at: i64) -> Result<String, String> {
    let time = chrono::DateTime::from_timestamp_millis(at).ok_or("Invalid calendar time")?;
    if !(1..=9999).contains(&chrono::Datelike::year(&time)) {
        return Err("Calendar year is outside the supported range".into());
    }
    Ok(time.format("%Y%m%dT%H%M%SZ").to_string())
}

pub fn calendar(title: &str, snapshot: &ExportSnapshot) -> Result<String, String> {
    let mut output = String::new();
    for header in [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//Lince//Schedule export//EN",
        "CALSCALE:GREGORIAN",
    ] {
        line(&mut output, header);
    }
    line(&mut output, &format!("X-WR-CALNAME:{}", text(title)));
    if snapshot.rows.is_empty() {
        for property in [
            "BEGIN:VTIMEZONE",
            "TZID:UTC",
            "BEGIN:STANDARD",
            "DTSTART:19700101T000000",
            "TZOFFSETFROM:+0000",
            "TZOFFSETTO:+0000",
            "END:STANDARD",
            "END:VTIMEZONE",
        ] {
            line(&mut output, property);
        }
    }
    let stamp = timestamp(snapshot.generated_at_ms)?;
    let mut identities = std::collections::HashSet::new();
    for row in &snapshot.rows {
        let id = row["uid"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("Schedule entry has no identity")?;
        if !identities.insert(id) {
            return Err("Duplicate calendar occurrence identity".into());
        }
        let head = row["head"].as_str().ok_or("Schedule entry has no name")?;
        let identity: String = Sha256::digest(format!("{}\0{id}", snapshot.scope))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        line(&mut output, "BEGIN:VEVENT");
        line(&mut output, &format!("UID:{identity}@lince.local"));
        line(&mut output, &format!("DTSTAMP:{stamp}"));
        line(&mut output, &format!("SUMMARY:{}", text(head)));
        if !row["time"].is_null() {
            let time: nucleus::schedule::TimeRange =
                serde_json::from_value(row["time"].clone()).map_err(|error| error.to_string())?;
            let start = timestamp(time.from_ms)?;
            line(&mut output, &format!("DTSTART:{start}"));
            if let Some(end) = time.until_ms {
                if end <= time.from_ms {
                    return Err("Calendar duration must be positive".into());
                }
                let end = timestamp(end)?;
                if end != start {
                    line(&mut output, &format!("DTEND:{end}"));
                }
            }
        } else {
            let start = row["start_date"]
                .as_str()
                .or(row["due_date"].as_str())
                .ok_or("Schedule entry has no time")?;
            let start = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
                .map_err(|error| error.to_string())?;
            let end = row["due_date"]
                .as_str()
                .map(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d"))
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or(start)
                .succ_opt()
                .ok_or("Calendar date overflow")?;
            if end <= start {
                return Err("Calendar date range is reversed".into());
            }
            if !(1..=9999).contains(&chrono::Datelike::year(&start))
                || !(1..=9999).contains(&chrono::Datelike::year(&end))
            {
                return Err("Calendar year is outside the supported range".into());
            }
            line(
                &mut output,
                &format!("DTSTART;VALUE=DATE:{}", start.format("%Y%m%d")),
            );
            line(
                &mut output,
                &format!("DTEND;VALUE=DATE:{}", end.format("%Y%m%d")),
            );
        }
        if row["origin"]["kind"] == "projection" {
            line(&mut output, "STATUS:TENTATIVE");
            line(
                &mut output,
                "DESCRIPTION:Simulated Karma occurrence. Lince regenerates this schedule.",
            );
        }
        line(&mut output, "TRANSP:TRANSPARENT");
        line(&mut output, "END:VEVENT");
    }
    line(&mut output, "END:VCALENDAR");
    Ok(output)
}

fn markdown(title: &str, snapshot: &ExportSnapshot) -> Result<String, String> {
    let mut output = format!("# {title}\n\n");
    for row in &snapshot.rows {
        let head = row["head"].as_str().ok_or("Record has no name")?;
        let body = row["body"].as_str().ok_or("Record has no body")?;
        output.push_str(&format!("## {head}\n\n{body}\n\n"));
    }
    Ok(output)
}

pub fn install(directory: &Path) -> Result<PathBuf, String> {
    let package = directory.join("plugins/lince.files");
    std::fs::create_dir_all(&package).map_err(|error| error.to_string())?;
    let executable = format!("lince-plugin-files{}", std::env::consts::EXE_SUFFIX);
    let staging = package.join(".executable.new");
    std::fs::copy(
        std::env::current_exe().map_err(|error| error.to_string())?,
        &staging,
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(staging, package.join(&executable)).map_err(|error| error.to_string())?;
    let manifest = manifest(executable);
    manifest.validate()?;
    std::fs::write(
        package.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(package)
}
