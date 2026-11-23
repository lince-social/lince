//! One replaceable local distance measurement for the tiny-scene profile. The sidecar is
//! derived data. Updating it never changes the authored HEAD or the accepted edit journal.
use super::*;
use nucleus::canvas::scene::{Handle, operation::Generation};
use nucleus::karma::DecimalValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistanceJob {
    pub generation: Generation,
    coordinates: [DecimalValue; 3],
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Measurement {
    job: DistanceJob,
    metres: Option<f64>,
}

fn state(dir: &Path) -> io::Result<Option<Measurement>> {
    match read(&dir.join("MEASUREMENT")) {
        Ok(state) => Ok(Some(state)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn write(dir: &Path, value: &Measurement) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(invalid)?;
    staged(dir, &bytes)?.persist(dir.join("MEASUREMENT"))?;
    File::open(dir)?.sync_all()
}

/// Capture the published source and order this generation before doing the calculation.
pub fn begin(
    dir: &Path,
    document: &DocumentId,
    revision: &CanonicalHash,
    handle: &Handle,
) -> io::Result<DistanceJob> {
    let _lock = lock(dir)?;
    let source = open(dir)?;
    if &source.scene.document != document || &source.revision != revision {
        return Err(invalid(
            "Measurement source revision changed. Reopen the scene.",
        ));
    }
    let slot = source
        .scene
        .slot(handle)
        .ok_or_else(|| invalid("Measurement placement is missing."))?;
    let job = DistanceJob {
        generation: Generation::capture(document.clone(), revision.clone(), handle.clone()),
        coordinates: slot.metres,
    };
    write(
        dir,
        &Measurement {
            job: job.clone(),
            metres: None,
        },
    )?;
    Ok(job)
}

/// Evaluate the captured coordinates outside the publication lock. Only the latest live
/// generation on the same source may replace its pending entry with the finished result.
pub fn finish(dir: &Path, job: &DistanceJob, scope_current: bool) -> io::Result<Option<f64>> {
    let [x, y, z] = job
        .coordinates
        .map(|v| v.mantissa() as f64 / 10_f64.powi(i32::from(v.scale())));
    let distance = x.hypot(y).hypot(z);
    if !scope_current {
        return Ok(None);
    }
    let _lock = lock(dir)?;
    let head: Head = read(&dir.join("HEAD"))?;
    if head.revision() != &job.generation.revision {
        return Ok(None);
    }
    let Some(mut entry) = state(dir)? else {
        return Ok(None);
    };
    if entry.job.generation != job.generation || entry.job.coordinates != job.coordinates {
        return Ok(None);
    }
    if let Some(metres) = entry.metres {
        return Ok(Some(metres));
    }
    entry.metres = Some(distance);
    write(dir, &entry)?;
    Ok(Some(distance))
}

pub fn cancel(dir: &Path, generation: &Generation) -> io::Result<()> {
    let _lock = lock(dir)?;
    if state(dir)?
        .is_some_and(|entry| entry.job.generation == *generation && entry.metres.is_none())
    {
        fs::remove_file(dir.join("MEASUREMENT"))?;
        File::open(dir)?.sync_all()?;
    }
    Ok(())
}

/// Recover a completed measurement only for the reopened placement at its accepted source.
pub fn recover(
    dir: &Path,
    document: &DocumentId,
    revision: &CanonicalHash,
    handle: &Handle,
) -> io::Result<Option<(DistanceJob, f64)>> {
    let head: Head = read(&dir.join("HEAD"))?;
    if head.revision() != revision {
        return Ok(None);
    }
    Ok(state(dir)?.and_then(|entry| {
        let g = &entry.job.generation;
        if &g.document == document && &g.revision == revision && &g.handle == handle {
            entry
                .metres
                .filter(|n| n.is_finite() && *n >= 0.)
                .map(|metres| (entry.job, metres))
        } else {
            None
        }
    }))
}
