use super::*;

impl Host {
    pub async fn install(&self, source: PathBuf) -> Result<(), String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || {
            let source = source
                .canonicalize()
                .map_err(|_| "Package directory is unavailable")?;
            let bytes = manifest_bytes(&source.join("plugin.json"))?;
            let manifest: Manifest =
                serde_json::from_slice(&bytes).map_err(|_| "Invalid plugin manifest")?;
            manifest.validate()?;
            let relative = Path::new(&manifest.executable);
            if relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err("Plugin executable must stay inside the package".to_string());
            }
            let executable = source
                .join(relative)
                .canonicalize()
                .map_err(|_| "Package executable is unavailable")?;
            if !executable.starts_with(&source) {
                return Err("Package executable escapes its directory".into());
            }
            package_digest(&executable, &source.join("plugin.json"))?;
            let target = root.join(&manifest.id);
            let staging = root.join(format!(".install-{}", manifest.id));
            let backup = root.join(format!(".previous-{}", manifest.id));
            if staging.exists() || backup.exists() {
                return Err("An interrupted package installation needs local review".into());
            }
            std::fs::create_dir(&staging).map_err(|error| error.to_string())?;
            let copy = (|| {
                let destination = staging.join(relative);
                std::fs::create_dir_all(destination.parent().ok_or("Invalid executable path")?)
                    .map_err(|error| error.to_string())?;
                std::fs::copy(&executable, &destination).map_err(|error| error.to_string())?;
                std::fs::write(staging.join("plugin.json"), bytes)
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(())
            })();
            if let Err(error) = copy {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(error);
            }
            let replacing = target.exists();
            if replacing {
                std::fs::rename(&target, &backup).map_err(|error| error.to_string())?;
            }
            if let Err(error) = std::fs::rename(&staging, &target) {
                if replacing {
                    let _ = std::fs::rename(&backup, &target);
                }
                let _ = std::fs::remove_dir_all(&staging);
                return Err(error.to_string());
            }
            if replacing {
                std::fs::remove_dir_all(backup).map_err(|error| error.to_string())?;
            }
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())??;
        self.reload().await
    }

    pub async fn remove(&self, id: &str, digest: &str) -> Result<(), String> {
        let installed = self
            .snapshot
            .read()
            .unwrap()
            .installed
            .get(id)
            .cloned()
            .ok_or("Package is unavailable")?;
        if installed.digest != digest {
            return Err("Package changed; inspect it before removal".into());
        }
        self.stop(id);
        let directory = installed
            .manifest_path
            .parent()
            .ok_or("Invalid package directory")?
            .to_path_buf();
        let hidden = self.directory.join(format!(".removed-{id}"));
        tokio::fs::rename(directory, &hidden)
            .await
            .map_err(|error| error.to_string())?;
        self.reload().await?;
        tokio::fs::remove_dir_all(hidden)
            .await
            .map_err(|error| error.to_string())
    }
}

pub fn install_current(directory: &Path, manifest: Manifest) -> Result<PathBuf, String> {
    manifest.validate()?;
    let package = directory.join("plugins").join(&manifest.id);
    std::fs::create_dir_all(&package).map_err(|error| error.to_string())?;
    let staging = package.join(".executable.new");
    std::fs::copy(
        std::env::current_exe().map_err(|error| error.to_string())?,
        &staging,
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(staging, package.join(&manifest.executable))
        .map_err(|error| error.to_string())?;
    let staging = package.join(".manifest.new");
    std::fs::write(
        &staging,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(staging, package.join("plugin.json")).map_err(|error| error.to_string())?;
    Ok(package)
}
