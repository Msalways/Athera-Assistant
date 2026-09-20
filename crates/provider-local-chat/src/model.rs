use assistant_contracts::conversation::{
    Cancellation, InstallationStatus, ModelInstallation, ModelManifest,
};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex as AsyncMutex,
    time::{sleep, Duration},
};

pub const QWEN3_1_7B_Q4_K_M: &str = "Qwen3-1.7B-Q4_K_M.gguf";
pub type InstallationSink = Arc<dyn Fn(ModelInstallation) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model installation was cancelled")]
    Cancelled,
    #[error("download failed: {0}")]
    Download(String),
    #[error("model size mismatch: expected {expected}, got {actual}")]
    Size { expected: u64, actual: u64 },
    #[error("model checksum mismatch")]
    Checksum,
    #[error("model storage failed: {0}")]
    Storage(#[from] std::io::Error),
}

pub fn default_manifest() -> ModelManifest {
    ModelManifest {
        id: "ggml-org/Qwen3-1.7B-GGUF:Q4_K_M".into(),
        url: concat!(
            "https://huggingface.co/ggml-org/Qwen3-1.7B-GGUF/resolve/",
            "daeb8e2d528a760970442092f6bf1e55c3b659eb/Qwen3-1.7B-Q4_K_M.gguf"
        )
        .into(),
        size_bytes: 1_282_439_264,
        sha256: "d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5".into(),
        revision: "daeb8e2d528a760970442092f6bf1e55c3b659eb".into(),
        license: "Apache-2.0".into(),
        runtime_revision: "llama.cpp-b10886".into(),
        context_tokens: 4_096,
    }
}

pub struct ModelManager {
    root: PathBuf,
    manifest: ModelManifest,
    client: Client,
    state: Mutex<ModelInstallation>,
    operation: AsyncMutex<()>,
}

impl ModelManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_manifest(root, default_manifest())
    }

    pub fn with_manifest(root: impl Into<PathBuf>, manifest: ModelManifest) -> Self {
        let root = root.into();
        let installed = root.join(QWEN3_1_7B_Q4_K_M).is_file()
            && std::fs::read_to_string(root.join(format!("{QWEN3_1_7B_Q4_K_M}.verified")))
                .is_ok_and(|value| value.trim() == manifest.sha256);
        let state = ModelInstallation {
            manifest: manifest.clone(),
            status: if installed {
                InstallationStatus::Installed
            } else {
                InstallationStatus::Missing
            },
            downloaded_bytes: if installed { manifest.size_bytes } else { 0 },
            error: None,
        };
        Self {
            root,
            manifest,
            client: Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .build()
                .expect("valid local model HTTP client"),
            state: Mutex::new(state),
            operation: AsyncMutex::new(()),
        }
    }

    pub fn model_path(&self) -> PathBuf {
        self.root.join(QWEN3_1_7B_Q4_K_M)
    }

    pub fn installation(&self) -> ModelInstallation {
        self.state.lock().expect("model state poisoned").clone()
    }

    pub async fn install(
        &self,
        cancel: Cancellation,
        sink: InstallationSink,
    ) -> Result<ModelInstallation, ModelError> {
        let _operation = self.operation.lock().await;
        let part = self.root.join(format!("{QWEN3_1_7B_Q4_K_M}.part"));
        let result = async {
            fs::create_dir_all(&self.root).await?;
            if self.verify_file(&self.model_path(), Some(&cancel)).await? {
                fs::write(self.verified_path(), format!("{}\n", self.manifest.sha256)).await?;
                return Ok(());
            }
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(ModelError::Cancelled);
            }
            let _ = fs::remove_file(self.model_path()).await;
            let _ = fs::remove_file(self.verified_path()).await;
            self.download(&part, &cancel, &sink).await?;
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(ModelError::Cancelled);
            }
            fs::rename(&part, self.model_path()).await?;
            fs::write(self.verified_path(), format!("{}\n", self.manifest.sha256)).await?;
            Ok::<(), ModelError>(())
        }
        .await;
        match result {
            Ok(()) => Ok(self.set_state(
                InstallationStatus::Installed,
                self.manifest.size_bytes,
                None,
                &sink,
            )),
            Err(error) => {
                let _ = fs::remove_file(&part).await;
                let status = if matches!(error, ModelError::Cancelled) {
                    InstallationStatus::Cancelled
                } else {
                    InstallationStatus::Failed
                };
                self.set_state(status, 0, Some(error.to_string()), &sink);
                Err(error)
            }
        }
    }

    async fn download(
        &self,
        part: &Path,
        cancel: &Cancellation,
        sink: &InstallationSink,
    ) -> Result<(), ModelError> {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ModelError::Cancelled);
        }
        self.set_state(InstallationStatus::Downloading, 0, None, sink);
        let mut response = network_wait(cancel, self.client.get(&self.manifest.url).send())
            .await?
            .error_for_status()
            .map_err(|error| ModelError::Download(error.to_string()))?;
        let mut file = fs::File::create(part).await?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0_u64;
        loop {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(ModelError::Cancelled);
            }
            let chunk = network_wait(cancel, response.chunk()).await?;
            let Some(chunk) = chunk else { break };
            let next = downloaded.saturating_add(chunk.len() as u64);
            if next > self.manifest.size_bytes {
                return Err(ModelError::Size {
                    expected: self.manifest.size_bytes,
                    actual: next,
                });
            }
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            downloaded = next;
            self.set_state(InstallationStatus::Downloading, downloaded, None, sink);
        }
        file.flush().await?;
        file.sync_all().await?;
        self.set_state(InstallationStatus::Verifying, downloaded, None, sink);
        if downloaded != self.manifest.size_bytes {
            return Err(ModelError::Size {
                expected: self.manifest.size_bytes,
                actual: downloaded,
            });
        }
        if format!("{:x}", hasher.finalize()) != self.manifest.sha256 {
            return Err(ModelError::Checksum);
        }
        Ok(())
    }

    pub async fn verify_installed(&self) -> Result<bool, ModelError> {
        let _operation = self.operation.lock().await;
        self.verify_file(&self.model_path(), None).await
    }

    async fn verify_file(
        &self,
        path: &Path,
        cancel: Option<&Cancellation>,
    ) -> Result<bool, ModelError> {
        let Ok(metadata) = fs::metadata(&path).await else {
            return Ok(false);
        };
        if metadata.len() != self.manifest.size_bytes {
            return Ok(false);
        }
        let mut file = fs::File::open(path).await?;
        let mut buffer = vec![0; 1024 * 1024];
        let mut hasher = Sha256::new();
        loop {
            if cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) {
                return Err(ModelError::Cancelled);
            }
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()) == self.manifest.sha256)
    }

    pub async fn remove(&self) -> Result<ModelInstallation, ModelError> {
        let _operation = self.operation.lock().await;
        let removed = self.root.join(format!("{QWEN3_1_7B_Q4_K_M}.removed"));
        match fs::rename(self.model_path(), &removed).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        for path in [
            removed,
            self.root.join(format!("{QWEN3_1_7B_Q4_K_M}.part")),
            self.verified_path(),
        ] {
            match fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let sink: InstallationSink = Arc::new(|_| {});
        Ok(self.set_state(InstallationStatus::Missing, 0, None, &sink))
    }

    fn verified_path(&self) -> PathBuf {
        self.root.join(format!("{QWEN3_1_7B_Q4_K_M}.verified"))
    }

    fn set_state(
        &self,
        status: InstallationStatus,
        downloaded_bytes: u64,
        error: Option<String>,
        sink: &InstallationSink,
    ) -> ModelInstallation {
        let next = ModelInstallation {
            manifest: self.manifest.clone(),
            status,
            downloaded_bytes,
            error,
        };
        *self.state.lock().expect("model state poisoned") = next.clone();
        sink(next.clone());
        next
    }
}

async fn network_wait<T>(
    cancel: &Cancellation,
    future: impl std::future::Future<Output = Result<T, reqwest::Error>>,
) -> Result<T, ModelError> {
    tokio::select! {
        result = future => result.map_err(|_| ModelError::Download("Network request failed".into())),
        _ = async { while !cancel.load(std::sync::atomic::Ordering::Acquire) { sleep(Duration::from_millis(50)).await; } } => Err(ModelError::Cancelled),
        _ = sleep(Duration::from_secs(60)) => Err(ModelError::Download("Network request timed out".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn remove_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "athera-model-test-{}",
            assistant_contracts::Id::new_v4()
        ));
        let manager = ModelManager::new(&root);
        assert_eq!(
            manager.remove().await.unwrap().status,
            InstallationStatus::Missing
        );
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn production_manifest_is_fully_pinned() {
        let manifest = default_manifest();
        assert_eq!(manifest.sha256.len(), 64);
        assert!(manifest.url.contains(&manifest.revision));
        assert_eq!(manifest.context_tokens, 4_096);
        assert!(manifest.size_bytes > 1_000_000_000);
    }
}
