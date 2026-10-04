use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    File(PathBuf),
    Url(String),
    Invalid(String),
}

impl Source {
    pub(super) fn parse(value: &str) -> Self {
        let value = value.trim();
        if value.starts_with("file:") {
            return reqwest::Url::parse(value)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .map(Self::File)
                .unwrap_or_else(|| {
                    Self::Invalid("Use a local file URL without a remote host.".into())
                });
        }
        if value.contains("://") || value.starts_with("data:") || value.starts_with("javascript:") {
            return match crate::media_sand::web_url(value) {
                Ok(url) => Self::Url(url.into()),
                Err(error) => Self::Invalid(error),
            };
        }
        if value.is_empty() || value.len() > 4096 || value.contains('\0') {
            Self::Invalid("Enter a file path or an HTTP(S) URL.".into())
        } else {
            Self::File(PathBuf::from(value))
        }
    }

    pub(super) fn from_path(path: PathBuf) -> Self {
        if let Some(value) = path.to_str()
            && (value.contains("://") || value.starts_with("file:"))
        {
            Self::parse(value)
        } else {
            Self::File(path)
        }
    }

    pub(super) fn name(&self) -> String {
        match self {
            Self::File(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            Self::Url(url) => url.clone(),
            Self::Invalid(error) => error.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Image,
    Document,
    Model,
    Text,
}

impl Kind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Image => "Image Sand",
            Self::Document => "Document Viewer Castle",
            Self::Model => "3D model",
            Self::Text => "Plain text Sand",
        }
    }
}

pub(super) fn kind(path: &Path) -> Result<Kind, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" => Ok(Kind::Image),
        "pdf" | "epub" => Ok(Kind::Document),
        "glb" | "gltf" | "gcloud" => Ok(Kind::Model),
        "txt" | "md" | "csv" | "json" | "toml" | "yaml" | "yml" | "rs" | "css" | "html" | "js" | "ts" | "wgsl" => Ok(Kind::Text),
        "svg" => Err("SVG has no file viewer here. Export PNG or JPEG for the Image Sand.".into()),
        "obj" | "fbx" | "stl" | "blend" => Err("The 3D importer accepts glTF 2.0 and GLB. Export this model as glTF or GLB.".into()),
        "ply" => Err("Convert Gaussian PLY to .gcloud with Lince’s Gaussian converter before importing.".into()),
        "doc" | "docx" | "odt" | "mobi" | "azw" | "azw3" => Err("The Document Viewer accepts PDF and EPUB. Export this document to PDF or EPUB.".into()),
        "mp3" | "mp4" | "wav" | "ogg" | "webm" | "flac" | "mov" => Err("Box has no standalone audio/video file player yet. The recorder does not open media files.".into()),
        "lingua" => Err("Dropping a .lingua file does not import Records. Use the explicit Record import or sync workflow.".into()),
        _ => Err(format!("No Sand accepts {}. Supported: PNG, JPEG, GIF, WebP, PDF, EPUB, GLB, glTF, .gcloud and small text files.", if extension.is_empty() { "this file format".to_owned() } else { format!(".{extension}") })),
    }
}

pub(super) fn mime_extension(mime: &str) -> Option<&'static str> {
    match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        "model/gltf-binary" => Some("glb"),
        "model/gltf+json" => Some("gltf"),
        "text/plain" => Some("txt"),
        _ => None,
    }
}
