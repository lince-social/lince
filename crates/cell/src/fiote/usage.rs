use super::*;
use nucleus::operation::Usage;
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn path(directory: &Path, thread: &str) -> PathBuf {
    directory.join(format!("usage-{thread}.json"))
}

pub(super) fn read(path: &Path) -> Result<Vec<Usage>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn record(path: &Path, report: Usage) -> Result<(), String> {
    if report.id.len() > 1024
        || report.source.len() > 1024
        || report.scope.len() > 128
        || report.cost.as_ref().is_some_and(|cost| {
            !cost.amount.is_finite()
                || cost.amount < 0.0
                || cost.currency.is_empty()
                || cost.currency.len() > 32
                || cost.currency.chars().any(char::is_control)
        })
    {
        return Err("The provider returned invalid usage metadata.".into());
    }
    let _writing = WRITING
        .lock()
        .map_err(|_| "Usage storage is unavailable.")?;
    let mut reports = read(path)?;
    if let Some(previous) = reports.iter_mut().find(|previous| {
        previous.id == report.id
            && previous.source == report.source
            && match (&previous.cost, &report.cost) {
                (Some(old), Some(new)) => old.currency == new.currency,
                _ => true,
            }
    }) {
        if *previous == report || previous.updated_ms > report.updated_ms {
            return Ok(());
        }
        *previous = report;
    } else {
        reports.push(report);
        if reports.len() > 512 {
            reports.remove(0);
        }
    }
    save(path, &reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_reports_replace_and_missing_cost_stays_missing() {
        let root = tempfile::tempdir().unwrap();
        let path = path(root.path(), "thread");
        let mut report = Usage {
            id: "session".into(),
            scope: "cumulative session".into(),
            context_used: Some(100),
            ..Default::default()
        };
        record(&path, report.clone()).unwrap();
        record(&path, report.clone()).unwrap();
        report.context_used = Some(50);
        record(&path, report).unwrap();
        let saved = read(&path).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].context_used, Some(50));
        assert!(saved[0].cost.is_none());
        record(
            &path,
            Usage::request("provider".into(), Some(1), Some(2), Some(3)),
        )
        .unwrap();
        assert_eq!(read(&path).unwrap().len(), 2);
    }
}
