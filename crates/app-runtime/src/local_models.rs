//! User-initiated model management. No download happens at startup.
use assistant_contracts::{conversation::*, Error, Result};
#[cfg(not(feature = "native-local-chat"))]
use provider_local_chat::LocalChatProvider;
use provider_local_chat::ModelManager;
#[cfg(feature = "native-local-chat")]
use provider_local_chat::NativeLocalChatProvider;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub(crate) struct LocalModels {
    pub provider: Arc<dyn ConversationProvider>,
    manager: Arc<ModelManager>,
    download: Arc<Mutex<Option<Cancellation>>>,
}

impl LocalModels {
    pub fn downloading(&self) -> bool {
        self.download.lock().map_or(true, |slot| slot.is_some())
    }
    pub fn new(root: PathBuf) -> Self {
        let manager = Arc::new(ModelManager::new(root));
        #[cfg(not(feature = "native-local-chat"))]
        let provider = {
            let executable = std::env::var_os("ASSISTANT_LLAMA_CLI")
                .map(PathBuf::from)
                .unwrap_or_default();
            Arc::new(LocalChatProvider::new(manager.model_path(), executable))
                as Arc<dyn ConversationProvider>
        };
        #[cfg(feature = "native-local-chat")]
        let provider = Arc::new(NativeLocalChatProvider::new(manager.model_path()))
            as Arc<dyn ConversationProvider>;
        Self {
            manager,
            provider,
            download: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn command(&self, name: &str, busy: bool) -> Result<Value> {
        match name {
            "model_status" => Ok(json!({"availability":self.provider.availability(),
                "installation":self.manager.installation(),"manifest":self.manager.installation().manifest})),
            "install_model" => {
                if busy {
                    return Err(Error::Conflict);
                }
                let mut download = self.download.lock().map_err(|_| Error::Unavailable)?;
                if download.is_some() {
                    return Err(Error::Conflict);
                }
                let cancel = Arc::new(AtomicBool::new(false));
                *download = Some(cancel.clone());
                let manager = self.manager.clone();
                let slot = self.download.clone();
                tokio::spawn(async move {
                    let _ = manager.install(cancel, Arc::new(|_| {})).await;
                    if let Ok(mut download) = slot.lock() {
                        *download = None;
                    }
                });
                Ok(Value::Null)
            }
            "cancel_model_download" => {
                if let Some(cancel) = self
                    .download
                    .lock()
                    .map_err(|_| Error::Unavailable)?
                    .as_ref()
                {
                    cancel.store(true, Ordering::Release);
                }
                Ok(Value::Null)
            }
            "remove_model" => {
                if busy
                    || self
                        .download
                        .lock()
                        .map_err(|_| Error::Unavailable)?
                        .is_some()
                {
                    return Err(Error::Conflict);
                }
                self.provider.unload().await?;
                self.manager
                    .remove()
                    .await
                    .map_err(|_| Error::Unavailable)?;
                Ok(Value::Null)
            }
            "unload_model" => {
                self.provider.unload().await?;
                Ok(Value::Null)
            }
            _ => Err(Error::InvalidInput),
        }
    }
}
