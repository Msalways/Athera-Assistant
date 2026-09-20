//! The only Rust module allowed to cross the native llama.cpp ABI.
use crate::provider::{LocalChatProvider, MODEL_BYTES, MODEL_SHA256};
use assistant_contracts::{
    conversation::{
        Cancellation, ConversationEvent, ConversationProvider, ConversationRequest,
        ConversationSink, ProviderAvailability,
    },
    Error, Result,
};
use async_trait::async_trait;
use std::{
    ffi::{c_char, c_void, CString},
    path::PathBuf,
    ptr::NonNull,
    sync::{Arc, Mutex},
};
use tokio::sync::Mutex as AsyncMutex;

#[repr(C)]
struct NativeHandle {
    _private: [u8; 0],
}

type TokenCallback = unsafe extern "C" fn(*const c_char, i32, *mut c_void) -> i32;
type CancelCallback = unsafe extern "C" fn(*mut c_void) -> i32;

extern "C" {
    fn athera_local_model_load(
        model_path: *const c_char,
        context_tokens: u32,
        error: *mut c_char,
        error_capacity: u32,
        cancel_callback: CancelCallback,
        user_data: *mut c_void,
    ) -> *mut NativeHandle;
    fn athera_local_generate(
        model: *mut NativeHandle,
        prompt: *const c_char,
        max_output_tokens: u32,
        token_callback: TokenCallback,
        cancel_callback: CancelCallback,
        user_data: *mut c_void,
    ) -> i32;
    fn athera_local_model_unload(model: *mut NativeHandle);
}

struct Model(NonNull<NativeHandle>);
unsafe impl Send for Model {}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe { athera_local_model_unload(self.0.as_ptr()) }
    }
}

struct CallbackState {
    sink: ConversationSink,
    cancel: Cancellation,
    pending: Vec<u8>,
}

unsafe extern "C" fn emit(bytes: *const c_char, length: i32, state: *mut c_void) -> i32 {
    if bytes.is_null() || length < 0 || state.is_null() {
        return 1;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let state = unsafe { &mut *(state.cast::<CallbackState>()) };
        let bytes = unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), length as usize) };
        state.pending.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&state.pending) {
            Ok(_) => state.pending.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => return 1,
        };
        if valid > 0 {
            let text =
                String::from_utf8(state.pending.drain(..valid).collect()).expect("validated UTF-8");
            (state.sink)(ConversationEvent::Delta { text });
        }
        0
    }))
    .unwrap_or(1)
}

unsafe extern "C" fn cancelled(state: *mut c_void) -> i32 {
    if state.is_null() {
        return 1;
    }
    std::panic::catch_unwind(|| {
        let state = unsafe { &*(state.cast::<CallbackState>()) };
        i32::from(state.cancel.load(std::sync::atomic::Ordering::Relaxed))
    })
    .unwrap_or(1)
}

unsafe extern "C" fn load_cancelled(state: *mut c_void) -> i32 {
    if state.is_null() {
        return 1;
    }
    std::panic::catch_unwind(|| {
        let cancel = unsafe { &*(state.cast::<Cancellation>()) };
        i32::from(cancel.load(std::sync::atomic::Ordering::Relaxed))
    })
    .unwrap_or(1)
}

pub struct NativeLocalChatProvider {
    model_path: PathBuf,
    model: Arc<Mutex<Option<Model>>>,
    generation: AsyncMutex<()>,
    active: Mutex<Option<Cancellation>>,
}

impl NativeLocalChatProvider {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            model: Arc::new(Mutex::new(None)),
            generation: AsyncMutex::new(()),
            active: Mutex::new(None),
        }
    }

    async fn ensure_loaded(&self, cancel: &Cancellation) -> Result<()> {
        if self.model.lock().map_err(|_| Error::Unavailable)?.is_some() {
            return Ok(());
        }
        let (size, hash) = LocalChatProvider::sha256(&self.model_path, Some(cancel)).await?;
        if size != MODEL_BYTES || hash != MODEL_SHA256 {
            return Err(Error::InvalidResponse);
        }
        let path = CString::new(self.model_path.to_string_lossy().as_bytes())
            .map_err(|_| Error::InvalidInput)?;
        let load_cancel = cancel.clone();
        let model = tokio::task::spawn_blocking(move || {
            let mut error = [0_i8; 256];
            let mut load_cancel = load_cancel;
            let pointer = unsafe {
                athera_local_model_load(
                    path.as_ptr(),
                    4_096,
                    error.as_mut_ptr(),
                    error.len() as u32,
                    load_cancelled,
                    (&mut load_cancel as *mut Cancellation).cast(),
                )
            };
            NonNull::new(pointer).map(Model)
        })
        .await
        .map_err(|_| Error::Unavailable)?;
        let model = model.ok_or(Error::Unavailable)?;
        *self.model.lock().map_err(|_| Error::Unavailable)? = Some(model);
        Ok(())
    }
}

#[async_trait]
impl ConversationProvider for NativeLocalChatProvider {
    fn availability(&self) -> ProviderAvailability {
        if !self.model_path.is_file() {
            ProviderAvailability::MissingModel
        } else if self.generation.try_lock().is_err() {
            ProviderAvailability::Busy
        } else {
            ProviderAvailability::Ready
        }
    }

    async fn generate(
        &self,
        request: ConversationRequest,
        cancel: Cancellation,
        sink: ConversationSink,
    ) -> Result<()> {
        if request.messages.is_empty() || request.max_output_tokens == 0 {
            return Err(Error::InvalidInput);
        }
        let _generation = self.generation.lock().await;
        *self.active.lock().map_err(|_| Error::Unavailable)? = Some(cancel.clone());
        let result = async {
            self.ensure_loaded(&cancel).await?;
            let prompt = CString::new(LocalChatProvider::prompt(&request))
                .map_err(|_| Error::InvalidInput)?;
            let models = self.model.clone();
            let limit = request.max_output_tokens;
            let finished = sink.clone();
            let (code, complete_utf8) = tokio::task::spawn_blocking(move || {
                let models = models.lock().map_err(|_| Error::Unavailable)?;
                let model = models.as_ref().ok_or(Error::Unavailable)?.0.as_ptr();
                let mut state = CallbackState {
                    sink,
                    cancel,
                    pending: Vec::new(),
                };
                let code = unsafe {
                    athera_local_generate(
                        model,
                        prompt.as_ptr(),
                        limit,
                        emit,
                        cancelled,
                        (&mut state as *mut CallbackState).cast(),
                    )
                };
                Ok::<_, Error>((code, state.pending.is_empty()))
            })
            .await
            .map_err(|_| Error::InvalidResponse)??;
            match code {
                0 if complete_utf8 => {
                    finished(ConversationEvent::Finished);
                    Ok(())
                }
                0 => Err(Error::InvalidResponse),
                1 => Err(Error::Denied),
                _ => Err(Error::InvalidResponse),
            }
        }
        .await;
        self.active.lock().map_err(|_| Error::Unavailable)?.take();
        result
    }

    async fn unload(&self) -> Result<()> {
        if let Some(cancel) = self.active.lock().map_err(|_| Error::Unavailable)?.as_ref() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let _generation = self.generation.lock().await;
        self.model.lock().map_err(|_| Error::Unavailable)?.take();
        Ok(())
    }
}
