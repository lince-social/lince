use super::source::{self, Kind, Source};
use crate::{media_sand::Pixels, topology::assets::ImportedAsset};
use bevy::prelude::*;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub(super) struct Package {
    path: PathBuf,
    keep: bool,
}

impl Drop for Package {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub(super) struct Prepared {
    pub path: PathBuf,
    pub kind: Kind,
    pub pixels: Option<Pixels>,
    pub text: Option<String>,
    pub asset: Option<ImportedAsset>,
    package: Option<Package>,
    download: Option<tempfile::TempDir>,
}

impl Prepared {
    pub(super) fn explanation(&self) -> String {
        match self.kind {
            Kind::Image => "Image Sand · PNG, JPEG, GIF and WebP. GIF displays its first frame. Display only; no Record is created.",
            Kind::Document => "Document Viewer Castle · PDF and EPUB, starting at the first page or chapter. Display only; no Record is created.",
            Kind::Model => "3D model · glTF 2.0, GLB or .gcloud. The package is copied into workspace assets when accepted. No Record is created.",
            Kind::Text => "Choose plain or editable text. Up to 4096 characters. HTML and code are displayed as text. No Record is created.",
        }.into()
    }

    pub(super) fn keep(&mut self) {
        if let Some(package) = &mut self.package {
            package.keep = true;
        }
        if self.asset.is_none()
            && let Some(download) = self.download.take()
        {
            let _ = download.keep();
        }
    }
}

pub(super) struct Job {
    pub root: Entity,
    pub id: u64,
    pub source: Source,
    pub directory: Option<PathBuf>,
    pub cancelled: Arc<AtomicBool>,
    pub wake: Option<crate::wake::WakeSignal>,
}

type Reply = (Entity, u64, Result<Prepared, String>);

#[derive(Resource)]
pub(super) struct Worker {
    pub sender: mpsc::SyncSender<Job>,
    pub replies: Mutex<mpsc::Receiver<Reply>>,
}

impl Default for Worker {
    fn default() -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Job>(4);
        let (output, replies) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("box-file-preview".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    if job.cancelled.load(Ordering::Acquire) {
                        continue;
                    }
                    let result = prepare(&job.source, job.directory.as_deref(), &job.cancelled);
                    if !job.cancelled.load(Ordering::Acquire)
                        && output.send((job.root, job.id, result)).is_err()
                    {
                        break;
                    }
                    if let Some(wake) = job.wake {
                        wake.ring();
                    }
                }
            })
            .expect("start Box file preview worker");
        Self {
            sender,
            replies: Mutex::new(replies),
        }
    }
}

pub(super) fn prepare(
    source: &Source,
    directory: Option<&Path>,
    cancelled: &AtomicBool,
) -> Result<Prepared, String> {
    let (path, download) = match source {
        Source::Invalid(error) => return Err(error.clone()),
        Source::File(path) => {
            if path
                .to_str()
                .is_none_or(|path| path.len() > 4096 || path.contains('\0'))
            {
                return Err("The file path must be valid UTF-8 and at most 4096 bytes.".into());
            }
            let kind = source::kind(path)?;
            if matches!(kind, Kind::Image | Kind::Document)
                && let Some(directory) = directory
            {
                let limit = if kind == Kind::Image {
                    crate::media_sand::MAX_BYTES
                } else {
                    lince_document::MAX_FILE_BYTES
                };
                std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                let storage = tempfile::Builder::new()
                    .prefix("box-file-")
                    .tempdir_in(directory)
                    .map_err(|error| error.to_string())?;
                let target = storage
                    .path()
                    .join(path.file_name().ok_or("Missing file name.")?);
                copy_file(path, &target, limit, cancelled)?;
                (target, Some(storage))
            } else {
                (path.clone(), None)
            }
        }
        Source::Url(url) => {
            let directory = directory.ok_or("Workspace asset storage is unavailable.")?;
            std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
            let download = tempfile::Builder::new()
                .prefix("box-download-")
                .tempdir_in(directory)
                .map_err(|error| error.to_string())?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let path = runtime.block_on(fetch(url, download.path(), cancelled))?;
            (path, Some(download))
        }
    };
    if cancelled.load(Ordering::Acquire) {
        return Err("Preview cancelled.".into());
    }
    let kind = source::kind(&path)?;
    let metadata =
        std::fs::metadata(&path).map_err(|error| format!("Cannot open file: {error}"))?;
    if !metadata.is_file() {
        return Err("Drop a supported file, rather than a folder.".into());
    }
    let mut prepared = Prepared {
        path,
        kind,
        pixels: None,
        text: None,
        asset: None,
        package: None,
        download,
    };
    match kind {
        Kind::Image => prepared.pixels = Some(crate::media_sand::decode(&prepared.path)?),
        Kind::Document => {
            if metadata.len() > lince_document::MAX_FILE_BYTES {
                return Err("Documents are limited to 256 MiB.".into());
            }
            let mut file =
                crate::media_sand::open_file(&prepared.path, lince_document::MAX_FILE_BYTES)?;
            let mut header = [0; 5];
            file.read_exact(&mut header)
                .map_err(|error| error.to_string())?;
            if &header != b"%PDF-" && &header[..2] != b"PK" {
                return Err("This file is not a PDF or EPUB.".into());
            }
        }
        Kind::Model => {
            let directory = directory.ok_or("Workspace asset storage is unavailable.")?;
            let asset = crate::topology::assets::copy_package(&prepared.path, directory, cancelled)
                .map_err(|error| {
                    if matches!(source, Source::Url(_)) && prepared.path.extension().is_some_and(|extension| extension == "gltf") {
                        format!("Cannot open this glTF: {error}. For URLs, use GLB or glTF with embedded resources. You can also drop the local glTF package.")
                    } else { error.to_string() }
                })?;
            prepared.package = Some(Package {
                path: directory.join(&asset.id),
                keep: false,
            });
            prepared.asset = Some(asset);
        }
        Kind::Text => {
            if metadata.len() > 16 * 1024 {
                return Err("Text Sands accept up to 4096 characters. Open larger files with the IDE Castle.".into());
            }
            let file = crate::media_sand::open_file(&prepared.path, 16 * 1024)?;
            let mut bytes = Vec::new();
            file.take(16 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            let text = String::from_utf8(bytes).map_err(|_| "The text file must use UTF-8.")?;
            if text.contains('\0') || text.chars().count() > 4096 {
                return Err("Text Sands accept UTF-8 text of up to 4096 characters.".into());
            }
            prepared.text = Some(text);
        }
    }
    Ok(prepared)
}

fn copy_file(
    source: &Path,
    target: &Path,
    limit: u64,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut source = crate::media_sand::open_file(source, limit)?;
    let mut target = std::fs::File::create(target).map_err(|error| error.to_string())?;
    let mut total = 0u64;
    let mut buffer = [0; 65536];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err("Preview cancelled.".into());
        }
        let count = source
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > limit {
            return Err("The file exceeds the viewer’s size limit.".into());
        }
        target
            .write_all(&buffer[..count])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn fetch(source: &str, directory: &Path, cancelled: &AtomicBool) -> Result<PathBuf, String> {
    let url = crate::media_sand::web_url(source)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3
                || crate::media_sand::web_url(attempt.url().as_str()).is_err()
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Cannot download file: {error}"))?
        .error_for_status()
        .map_err(|error| format!("The file server returned an error: {error}"))?;
    let path = Path::new(response.url().path());
    let extension = if source::kind(path).is_ok() {
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap()
            .to_ascii_lowercase()
    } else {
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(source::mime_extension)
            .ok_or("This URL does not return a supported file. Use Link Sand for web pages.")?
            .to_owned()
    };
    let kind = source::kind(Path::new(&format!("source.{extension}")))?;
    let limit = match kind {
        Kind::Image => crate::media_sand::MAX_BYTES,
        Kind::Document => lince_document::MAX_FILE_BYTES,
        Kind::Model => 128 * 1024 * 1024,
        Kind::Text => 16 * 1024,
    };
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err(format!(
            "This download exceeds the {limit} byte viewer limit."
        ));
    }
    let path = directory.join(format!("source.{extension}"));
    let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
    let mut total = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if cancelled.load(Ordering::Acquire) {
            return Err("Preview cancelled.".into());
        }
        total = total.saturating_add(chunk.len() as u64);
        if total > limit {
            return Err("The download exceeds the viewer’s size limit.".into());
        }
        file.write_all(&chunk).map_err(|error| error.to_string())?;
    }
    Ok(path)
}
