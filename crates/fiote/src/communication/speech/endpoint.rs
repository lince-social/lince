use crate::config::Secret;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub url: String,
    pub model: String,
    pub allow_cloud: bool,
    pub requires_key: bool,
}

impl Settings {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.url.len() > 8192 {
            return Err("The transcription endpoint URL exceeds 8 KiB.".into());
        }
        let mut settings = crate::config::Settings {
            endpoint: self.url.clone(),
            model: self.model.clone(),
            ..Default::default()
        };
        settings.validate()?;
        if settings.endpoint.is_empty() || settings.model.is_empty() {
            return Err("Choose a transcription endpoint URL and model.".into());
        }
        let url =
            url::Url::parse(&settings.endpoint).map_err(|_| "Invalid transcription endpoint.")?;
        let local = url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"));
        if !local && !self.allow_cloud {
            return Err("Allow cloud transcription before using a remote endpoint.".into());
        }
        self.url = url.as_str().trim_end_matches('/').into();
        self.model = settings.model;
        Ok(())
    }

    pub fn slot(&self) -> String {
        format!("speech:{}", self.url)
    }
}

pub async fn transcribe(
    mut settings: Settings,
    credential: Option<Secret>,
    audio: Secret,
) -> Result<String, String> {
    settings.validate()?;
    super::validate_audio(&audio.0)?;
    if settings.requires_key && credential.as_ref().is_none_or(|key| key.0.is_empty()) {
        return Err(
            "Unlock the vault and configure the transcription API key. No audio was sent.".into(),
        );
    }
    let bytes = BASE64
        .decode(&audio.0)
        .map_err(|_| "Invalid dictation audio.")?;
    let file = reqwest::multipart::Part::bytes(bytes)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|_| "Cannot prepare microphone audio.")?;
    let form = reqwest::multipart::Form::new()
        .text("model", settings.model)
        .text("response_format", "json")
        .part("file", file);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|_| "Cannot create the transcription connection.")?;
    let mut request = client.post(&settings.url).multipart(form);
    if let Some(key) = credential {
        if !key.0.is_empty() {
            request = request.bearer_auth(&key.0);
        }
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| "Transcription connection failed; your draft is unchanged.")?;
    if !response.status().is_success() {
        return Err(format!(
            "Transcription endpoint rejected the request (HTTP {}). Your draft is unchanged.",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "The transcription response was interrupted.")?
    {
        if bytes.len().saturating_add(chunk.len()) > 131_072 {
            return Err("The transcription response exceeds its size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let result: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The transcription endpoint did not return a JSON transcript.")?;
    transcript(&result)
}

pub fn transcript(value: &serde_json::Value) -> Result<String, String> {
    let text = value["text"]
        .as_str()
        .ok_or("The transcription endpoint returned no text.")?
        .trim();
    if text.is_empty() {
        return Err("No speech was recognized. Your draft is unchanged.".into());
    }
    if text.len() > 65_536 {
        return Err("The transcript exceeds its size limit.".into());
    }
    Ok(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compatible_http_endpoint_receives_original_wav_and_returns_editable_text() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let audio = Secret(
            "UklGRjQAAABXQVZFZm10IBAAAAABAAEAgD4AAAB9AAACABAAZGF0YRAAAAABAQEBAQEBAQEBAQEBAQEB"
                .into(),
        );
        let original = BASE64.decode(&audio.0).unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let read = socket.read(&mut chunk).await.unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&chunk[..read]);
                if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8(bytes[..offset].to_vec()).unwrap();
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= offset + 4 + length {
                        assert!(header.starts_with("POST /audio/transcriptions "));
                        assert!(!header.to_ascii_lowercase().contains("authorization:"));
                        assert!(header.contains("multipart/form-data"));
                        break;
                    }
                }
                assert!(bytes.len() <= 65536);
            }
            assert!(bytes.windows(original.len()).any(|part| part == original));
            let multipart = String::from_utf8_lossy(&bytes);
            assert!(multipart.contains("recording.wav"));
            assert!(multipart.contains("audio/wav"));
            assert!(multipart.contains("local-transcriber"));
            assert!(multipart.contains("response_format"));
            let body = r#"{"text":"  Fixture recognized words  "}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let mut settings = Settings {
            url: format!("http://{address}/audio/transcriptions"),
            model: "local-transcriber".into(),
            allow_cloud: false,
            requires_key: true,
        };
        assert!(
            transcribe(settings.clone(), None, audio.clone())
                .await
                .unwrap_err()
                .contains("No audio was sent")
        );
        assert!(!server.is_finished());
        settings.requires_key = false;
        let text = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            transcribe(settings, None, audio),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(text, "Fixture recognized words");
        server.await.unwrap();
    }

    #[test]
    fn endpoint_cloud_credentials_and_transcript_bounds_are_explicit() {
        let mut settings = Settings {
            url: "http://127.0.0.1:8080/audio/transcriptions".into(),
            model: "local".into(),
            allow_cloud: false,
            requires_key: false,
        };
        settings.validate().unwrap();
        assert_eq!(settings.url, "http://127.0.0.1:8080/audio/transcriptions");
        settings.url = "https://example.test/audio/transcriptions".into();
        assert!(settings.validate().is_err());
        settings.allow_cloud = true;
        settings.validate().unwrap();
        settings.url = "https://key@example.test/audio/transcriptions".into();
        assert!(settings.validate().is_err());
        assert_eq!(
            transcript(&serde_json::json!({"text":" hello "})).unwrap(),
            "hello"
        );
        assert!(transcript(&serde_json::json!({"text":" "})).is_err());
        assert!(transcript(&serde_json::json!({"text":"a".repeat(65_537)})).is_err());
    }
}
