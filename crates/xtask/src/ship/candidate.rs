use super::{Tools, output};
use crate::Result;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const ASSET: &str = "lince-x86_64-unknown-linux-gnu.tar.gz";
const TARGET: &str = "x86_64-unknown-linux-gnu";

const EXTRACT: &str = r#"
import pathlib, shutil, sys, tarfile
archive, destination = sys.argv[1:]
root = pathlib.Path(destination)
with tarfile.open(archive, 'r:gz') as tar:
    members = []
    total = 0
    seen = set()
    for member in tar:
        if len(members) >= 16384:
            raise ValueError('Too many archive entries')
        members.append(member)
        path = pathlib.PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != 'lince':
            raise ValueError('Invalid archive path')
        if not (member.isfile() or member.isdir()) or member.name in seen:
            raise ValueError('Links, special files and duplicate entries are forbidden')
        seen.add(member.name)
        total += member.size
        if member.size < 0 or total > 8 * 1024 ** 3:
            raise ValueError('Archive exceeds extraction limit')
    for member in members:
        path = root.joinpath(*pathlib.PurePosixPath(member.name).parts)
        if member.isdir():
            path.mkdir(parents=True, exist_ok=True, mode=0o700)
        else:
            path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            with tar.extractfile(member) as source, path.open('xb') as target:
                shutil.copyfileobj(source, target)
            path.chmod(0o700 if member.mode & 0o111 else 0o600)
"#;

fn verify_digest(archive: &Path) -> Result<()> {
    let sidecar = archive.with_extension("gz.sha256");
    let contents = fs::read_to_string(&sidecar).map_err(|error| error.to_string())?;
    let fields: Vec<_> = contents.split_whitespace().collect();
    if fields.len() != 2
        || fields[0].len() != 64
        || !fields[0].bytes().all(|byte| byte.is_ascii_hexdigit())
        || fields[1] != ASSET
    {
        return Err("Invalid candidate checksum sidecar".into());
    }
    let mut file = fs::File::open(archive).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if format!("{:x}", digest.finalize()) != fields[0].to_ascii_lowercase() {
        return Err("Candidate archive checksum mismatch".into());
    }
    Ok(())
}

fn verify_info(info: &serde_json::Value, revision: &str) -> Result<()> {
    if info["revision"].as_str() != Some(revision)
        || info["target"].as_str() != Some(TARGET)
        || info["optimized"] != true
    {
        return Err(
            "Candidate revision, target or optimization does not match the requested Linux release"
                .into(),
        );
    }
    for feature in ["ui", "instinct", "park"] {
        if info["features"][feature] != true {
            return Err(format!("Candidate is missing required feature {feature}"));
        }
    }
    Ok(())
}

pub(super) fn realize(
    tools: &Tools,
    source: &Path,
    state: &Path,
    repository: &str,
    run_id: u64,
    revision: &str,
) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = state.join("lince-candidates").join(format!(
        "{run_id}-{revision}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(directory.parent().ok_or("Missing candidate parent")?)
        .map_err(|error| error.to_string())?;
    fs::create_dir(&directory)
        .map_err(|error| format!("Cannot create fresh candidate directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    let result = (|| {
        let artifacts = directory.join("artifacts");
        output(
            Command::new(&tools.gh)
                .args([
                    "run",
                    "download",
                    &run_id.to_string(),
                    "--repo",
                    repository,
                    "--name",
                    "lince-binary-artifacts-x86_64-unknown-linux-gnu",
                    "--dir",
                ])
                .arg(&artifacts),
        )?;
        let archive = artifacts.join(ASSET);
        verify_digest(&archive)?;
        let unpacked = directory.join("unpacked");
        fs::create_dir(&unpacked).map_err(|error| error.to_string())?;
        output(
            Command::new("python3")
                .args(["-B", "-c", EXTRACT])
                .arg(&archive)
                .arg(&unpacked),
        )?;
        let bundle = unpacked.join("lince");
        let info: serde_json::Value = serde_json::from_slice(
            &fs::read(bundle.join("build-info.json")).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        verify_info(&info, revision)?;
        let root_link = directory.join("package");
        let built = output(
            Command::new(&tools.nix)
                .args([
                    "build",
                    "--print-out-paths",
                    "--no-write-lock-file",
                    "--out-link",
                ])
                .arg(&root_link)
                .arg(format!("path:{}#lince-park", source.display()))
                .args(["--override-input", "lince-binary"])
                .arg(format!("path:{}", bundle.display())),
        )?;
        let lines: Vec<_> = built.lines().collect();
        if lines.len() != 1
            || !lines[0].starts_with("/nix/store/")
            || lines[0].split('/').count() != 4
        {
            return Err("Nix did not return one immutable store package".into());
        }
        let executable = PathBuf::from(lines[0]).join("bin/lince");
        let actual: serde_json::Value = serde_json::from_str(&output(
            Command::new("timeout")
                .arg("30")
                .arg(&executable)
                .arg("--build-info"),
        )?)
        .map_err(|error| error.to_string())?;
        verify_info(&actual, revision)?;
        fs::write(directory.join("candidate.json"), serde_json::to_vec_pretty(&serde_json::json!({"run_id": run_id, "revision": revision, "executable": executable, "active": false, "swap_complete": false})).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        fs::remove_dir_all(&artifacts).map_err(|error| error.to_string())?;
        fs::remove_dir_all(&unpacked).map_err(|error| error.to_string())?;
        Ok(executable)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(directory);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_identity_requires_the_exact_optimized_feature_complete_build() {
        let expected = serde_json::json!({"revision":"wanted", "target":TARGET, "optimized":true, "features":{"ui":true,"instinct":true,"park":true}});
        verify_info(&expected, "wanted").unwrap();
        assert!(verify_info(&expected, "different").is_err());
        for field in ["optimized", "features"] {
            let mut value = expected.clone();
            value[field] = serde_json::Value::Null;
            assert!(verify_info(&value, "wanted").is_err());
        }
        for feature in ["ui", "instinct", "park"] {
            let mut value = expected.clone();
            value["features"][feature] = false.into();
            assert!(verify_info(&value, "wanted").is_err());
        }
    }

    #[test]
    fn checksum_validation_rejects_changed_bytes_and_wrong_asset_names() {
        let repository = super::super::tests::Repository::new();
        let archive = repository.directory.join(ASSET);
        fs::write(&archive, b"candidate").unwrap();
        let checksum = format!("{:x}", Sha256::digest(b"candidate"));
        fs::write(
            archive.with_extension("gz.sha256"),
            format!("{checksum}  {ASSET}\n"),
        )
        .unwrap();
        verify_digest(&archive).unwrap();
        fs::write(&archive, b"changed").unwrap();
        assert!(verify_digest(&archive).unwrap_err().contains("mismatch"));
        fs::write(
            archive.with_extension("gz.sha256"),
            format!("{checksum}  other.tar.gz\n"),
        )
        .unwrap();
        assert!(verify_digest(&archive).is_err());
    }

    #[test]
    fn extraction_rejects_links_and_path_traversal() {
        let repository = super::super::tests::Repository::new();
        let archive = repository.directory.join("malicious.tar.gz");
        let destination = repository.directory.join("unpacked");
        fs::create_dir(&destination).unwrap();
        for name in [
            "../outside",
            "/absolute",
            "lince/../../outside",
            "other/bin/lince",
        ] {
            output(Command::new("python3").args(["-B", "-c", "import sys,tarfile; t=tarfile.open(sys.argv[1],'w:gz'); t.addfile(tarfile.TarInfo(sys.argv[2])); t.close()"]).arg(&archive).arg(name)).unwrap();
            assert!(
                output(
                    Command::new("python3")
                        .args(["-B", "-c", EXTRACT])
                        .arg(&archive)
                        .arg(&destination)
                )
                .is_err()
            );
        }
        output(Command::new("python3").args(["-B", "-c", "import sys,tarfile; t=tarfile.open(sys.argv[1],'w:gz'); m=tarfile.TarInfo('lince/link'); m.type=tarfile.SYMTYPE; m.linkname='/tmp'; t.addfile(m); t.close()"]).arg(&archive)).unwrap();
        assert!(
            output(
                Command::new("python3")
                    .args(["-B", "-c", EXTRACT])
                    .arg(&archive)
                    .arg(&destination)
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}
