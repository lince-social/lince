use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

const REVISION: &str = "47534e27c9851bb1128ccc0102f1145e27f23f98";
const MODELS: &[(&str, &str, &str)] = &[
    (
        "face_detection_yunet",
        "face_detection_yunet_2023mar.onnx",
        "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
    ),
    (
        "palm_detection_mediapipe",
        "palm_detection_mediapipe_2023feb.onnx",
        "78ff51c38496b7fc8b8ebdb6cc8c1abb02fa6c38427c6848254cdaba57fcce7c",
    ),
    (
        "handpose_estimation_mediapipe",
        "handpose_estimation_mediapipe_2023feb.onnx",
        "db0898ae717b76b075d9bf563af315b29562e11f8df5027a1ef07b02bef6d81c",
    ),
];

pub(super) fn download(root: &Path) -> super::Result<()> {
    tokio::runtime::Runtime::new()
        .map_err(|error| error.to_string())?
        .block_on(async {
            let directory = root.join(".cache-lince-hand");
            fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|error| error.to_string())?;
            for &(folder, filename, checksum) in MODELS {
                let path = directory.join(filename);
                if fs::read(&path).is_ok_and(|bytes| valid(&bytes, checksum)) {
                    println!("Already verified: {}", path.display());
                    continue;
                }
                let url = format!(
                    "https://media.githubusercontent.com/media/opencv/opencv_zoo/{REVISION}/models/{folder}/{filename}"
                );
                let bytes = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|error| error.to_string())?
                    .error_for_status()
                    .map_err(|error| error.to_string())?
                    .bytes()
                    .await
                    .map_err(|error| error.to_string())?;
                if !valid(&bytes, checksum) {
                    return Err(format!("Checksum mismatch for {filename}"));
                }
                let temporary = path.with_extension("onnx.part");
                fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
                fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
                println!("Downloaded and verified: {}", path.display());
            }
            Ok(())
        })
}

fn valid(bytes: &[u8], expected: &str) -> bool {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
        == expected
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_corrupt_model_downloads() {
        assert!(!super::valid(b"not an ONNX model", super::MODELS[0].2));
        assert!(super::valid(
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
    }
}
