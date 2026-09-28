//! Ordered, bounded, attributed provider failover.
//!
//! A chain is only walked for failures that are transient and
//! provider-independent (see `failover::may_fail_over`). Configuration and
//! capability facts - a rejected key, an unknown model, a bad endpoint, or a
//! stream the provider could not produce - are returned to the user instead of
//! being hidden by quietly sending the prompt to another vendor.
//!
//! The chain is bounded to one fallback attempt, and the provider that actually
//! answered is recorded so the turn stays attributable.
//!
//! A chain member may be local (`ModelCapabilities::local`). Moving to a local
//! model is a capability downgrade, not an equivalent retry, so it is only taken
//! automatically for work that is read-only; see `local_fallback_allowed`.

use assistant_contracts::failover::{may_fail_over, FAILOVER_MAX_ATTEMPTS};
use assistant_contracts::{
    AgentAction, ContextBundle, Error, ModelCapabilities, ModelProvider, ProviderEvent,
    ProviderEventSink, Result,
};
use provider_rig::cloud::RigCloudProvider;
use provider_rig::RigAdapterError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Why a local fallback was or was not taken, for the caller to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalFallback {
    /// No local member exists, so there was nothing to fall back to.
    Unavailable,
    /// The local member was not tried because the work is not read-only.
    NeedsConsent,
}

pub struct FailoverCloud {
    chain: Vec<Arc<dyn ModelProvider>>,
    served_by: Arc<Mutex<Option<String>>>,
    local_fallback: Arc<Mutex<Option<LocalFallback>>>,
}

/// A downgrade to a model that runs on this device is only taken on its own for
/// work that cannot change anything. Anything that writes externally, reads
/// sensitive data, or destroys state must be agreed to by the user first,
/// because the local model is a different system with different limits and
/// agreeing to a cloud request is not agreeing to a local one.
pub fn local_fallback_allowed(context: &ContextBundle) -> bool {
    context.tools.iter().all(|tool| tool.risk.retry_safe())
}

impl FailoverCloud {
    pub fn new(chain: Vec<Arc<dyn ModelProvider>>) -> Self {
        Self::with_attribution(chain, Arc::new(Mutex::new(None)))
    }

    /// Build a chain that records the answering provider into a handle the
    /// runtime can surface, so a fallback is never invisible.
    pub fn with_attribution(
        chain: Vec<Arc<dyn ModelProvider>>,
        served_by: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            chain,
            served_by,
            local_fallback: Arc::new(Mutex::new(None)),
        }
    }

    /// The provider that most recently produced an answer, if any.
    pub fn served_by(&self) -> Option<String> {
        self.served_by.lock().ok().and_then(|v| v.clone())
    }

    /// Whether the last attempt ended in a blocked or absent local fallback.
    pub fn local_fallback(&self) -> Option<LocalFallback> {
        self.local_fallback.lock().ok().and_then(|v| v.clone())
    }

    /// Whether the member at `index` runs on this device.
    pub fn is_local(&self, index: usize) -> bool {
        self.chain
            .get(index)
            .is_some_and(|provider| provider.capabilities().local)
    }

    pub fn ids(&self) -> Vec<String> {
        self.chain
            .iter()
            .map(|provider| provider.id().to_owned())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.chain.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chain.is_empty()
    }

    async fn attempt(
        &self,
        context: ContextBundle,
        sink: Option<ProviderEventSink>,
    ) -> Result<AgentAction> {
        // Two budgets, not one. Vendors are capped so a turn cannot fan out across
        // many of them, and the on-device member gets its own single attempt
        // because it sits behind the vendors. Capping the whole chain at the
        // vendor budget instead would place the local member permanently out of
        // reach and make opting into it a no-op.
        let cloud_count = self
            .chain
            .iter()
            .filter(|provider| !provider.capabilities().local)
            .count();
        let cloud_limit = cloud_count.min(FAILOVER_MAX_ATTEMPTS);
        let read_only = local_fallback_allowed(&context);
        let mut primary_error: Option<Error> = None;
        let mut cloud_seen = 0usize;
        let mut local_seen = 0usize;
        for (index, provider) in self.chain.iter().enumerate() {
            let local = provider.capabilities().local;
            if local {
                if local_seen >= 1 {
                    continue;
                }
                if !read_only {
                    // Refuse the downgrade rather than act outside what was agreed.
                    if let Ok(mut slot) = self.local_fallback.lock() {
                        *slot = Some(LocalFallback::NeedsConsent);
                    }
                    continue;
                }
                if !provider.is_available() {
                    if let Ok(mut slot) = self.local_fallback.lock() {
                        *slot = Some(LocalFallback::Unavailable);
                    }
                    continue;
                }
                local_seen += 1;
            } else {
                if cloud_seen >= cloud_limit {
                    continue;
                }
                cloud_seen += 1;
            }
            // A provider that already produced visible text must not be replaced:
            // the user would see one vendor's words flow into another's. Track it
            // so a mid-stream fault surfaces instead of silently switching.
            let streamed = Arc::new(AtomicBool::new(false));
            let observed = sink.as_ref().map(|inner| {
                let observed = streamed.clone();
                let inner = inner.clone();
                Arc::new(move |event: ProviderEvent| {
                    if matches!(event, ProviderEvent::TextDelta { .. }) {
                        observed.store(true, Ordering::Release);
                    }
                    inner(event)
                }) as ProviderEventSink
            });
            if let Some(observed) = observed.as_ref() {
                let _ = observed(ProviderEvent::ProviderSelected {
                    provider_id: provider.id().to_owned(),
                });
            }
            match provider.infer_typed(context.clone(), observed).await {
                Ok(action) => {
                    if let Ok(mut served) = self.served_by.lock() {
                        *served = Some(provider.id().to_owned());
                    }
                    return Ok(action);
                }
                Err(normalized) => {
                    // The primary's own failure is what the user should see.
                    if index == 0 {
                        primary_error = Some(RigCloudProvider::engine_error(
                            RigAdapterError::Normalized(normalized.clone()),
                        ));
                    }
                    if !may_fail_over(&normalized) {
                        break;
                    }
                    if streamed.load(Ordering::Acquire) {
                        // Text from this provider is already on screen.
                        break;
                    }
                }
            }
        }
        Err(primary_error.unwrap_or(Error::Unavailable))
    }
}

#[async_trait::async_trait]
impl ModelProvider for FailoverCloud {
    fn id(&self) -> &str {
        self.chain
            .first()
            .map(|provider| provider.id())
            .unwrap_or("failover")
    }

    fn capabilities(&self) -> ModelCapabilities {
        self.chain
            .first()
            .map(|provider| provider.capabilities())
            .unwrap_or(ModelCapabilities {
                tool_calls: false,
                planning: false,
                local: false,
            })
    }

    fn is_available(&self) -> bool {
        !self.chain.is_empty() && self.chain.iter().any(|p| p.is_available())
    }

    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        self.attempt(context, None).await
    }

    async fn infer_stream(
        &self,
        context: ContextBundle,
        sink: ProviderEventSink,
    ) -> Result<AgentAction> {
        self.attempt(context, Some(sink)).await
    }
}
