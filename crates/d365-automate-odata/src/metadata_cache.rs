//! In-memory TTL cache for service metadata.
//!
//! A decorator over any [`D365Client`]: it intercepts `service_metadata` and
//! `bulk_service_metadata`, serves hits from a key-`(operation, language)`
//! map, and falls through to the inner client on miss. In-memory only; TTL is
//! the single eviction policy. The inner map is a `tokio::sync::RwLock`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::client::{
    BulkMetadata, D365Client, EntityRow, EntityStructure, EnvironmentInfo, PoolStatus,
    ReadEntityRequest, ServiceCallRequest, ServiceOperationMeta, ServiceSearchResult,
};
use crate::error::D365Result;

/// Cache statistics surfaced to Prometheus / TUI.
#[derive(Debug, Default, Clone, Copy)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
}

impl CacheStats {
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    meta: ServiceOperationMeta,
    cached_at: Instant,
}

/// Decorator that caches `ServiceOperationMeta` keyed by `(operation, language)`
/// for a configurable TTL.
pub struct MetadataCache<C: D365Client + ?Sized> {
    inner: Arc<C>,
    ttl: Duration,
    entries: RwLock<HashMap<(String, String), Entry>>,
    stats: RwLock<CacheStats>,
}

impl<C: D365Client + ?Sized> MetadataCache<C> {
    /// Wrap `inner` with a TTL cache.  TTL of 0 disables caching.
    pub fn new(inner: Arc<C>, ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner,
            ttl,
            entries: RwLock::new(HashMap::new()),
            stats: RwLock::new(CacheStats::default()),
        })
    }

    pub async fn stats(&self) -> CacheStats {
        let mut s = *self.stats.read().await;
        s.entries = self.entries.read().await.len();
        s
    }

    /// Drop every entry.  Useful on environment-role flip or after a
    /// metadata-changing deployment (which may have changed signatures).
    pub async fn invalidate_all(&self) {
        let mut entries = self.entries.write().await;
        let evicted = entries.len() as u64;
        entries.clear();
        self.stats.write().await.evictions += evicted;
    }

    async fn get_fresh(&self, key: &(String, String)) -> Option<ServiceOperationMeta> {
        let entries = self.entries.read().await;
        let e = entries.get(key)?;
        if self.ttl.is_zero() || e.cached_at.elapsed() <= self.ttl {
            Some(e.meta.clone())
        } else {
            None
        }
    }

    async fn store(&self, key: (String, String), meta: ServiceOperationMeta) {
        if self.ttl.is_zero() {
            return;
        }
        let mut entries = self.entries.write().await;
        entries.insert(
            key,
            Entry {
                meta,
                cached_at: Instant::now(),
            },
        );
    }
}

#[async_trait]
impl<C: D365Client + ?Sized> D365Client for MetadataCache<C> {
    async fn environment_info(&self) -> D365Result<EnvironmentInfo> {
        self.inner.environment_info().await
    }

    async fn search_service(&self, query: &str, limit: usize) -> D365Result<ServiceSearchResult> {
        self.inner.search_service(query, limit).await
    }

    async fn service_metadata(
        &self,
        operation: &str,
        language: &str,
    ) -> D365Result<ServiceOperationMeta> {
        let key = (operation.to_string(), language.to_string());
        if let Some(meta) = self.get_fresh(&key).await {
            self.stats.write().await.hits += 1;
            return Ok(meta);
        }
        self.stats.write().await.misses += 1;
        let meta = self.inner.service_metadata(operation, language).await?;
        self.store(key, meta.clone()).await;
        Ok(meta)
    }

    async fn bulk_service_metadata(
        &self,
        operations: &[String],
        language: &str,
    ) -> D365Result<BulkMetadata> {
        // Serve as many as are cached; fall through for the rest in one bulk call.
        let mut cached = Vec::new();
        let mut to_fetch = Vec::new();
        for op in operations {
            let key = (op.clone(), language.to_string());
            if let Some(meta) = self.get_fresh(&key).await {
                self.stats.write().await.hits += 1;
                cached.push(meta);
            } else {
                self.stats.write().await.misses += 1;
                to_fetch.push(op.clone());
            }
        }
        let mut missing = Vec::new();
        if !to_fetch.is_empty() {
            let fetched = self
                .inner
                .bulk_service_metadata(&to_fetch, language)
                .await?;
            for meta in &fetched.operations {
                self.store((meta.operation.clone(), language.to_string()), meta.clone())
                    .await;
            }
            cached.extend(fetched.operations);
            missing = fetched.missing;
        }
        Ok(BulkMetadata {
            language: language.to_string(),
            operations: cached,
            missing,
        })
    }

    async fn call_service(
        &self,
        request: ServiceCallRequest,
        read_only_mode: bool,
    ) -> D365Result<serde_json::Value> {
        self.inner.call_service(request, read_only_mode).await
    }

    async fn read_entity(&self, request: ReadEntityRequest) -> D365Result<Vec<EntityRow>> {
        self.inner.read_entity(request).await
    }

    async fn entity_structure(&self, entity: &str) -> D365Result<EntityStructure> {
        self.inner.entity_structure(entity).await
    }

    fn pool_status(&self) -> PoolStatus {
        self.inner.pool_status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockD365Client;
    use serde_json::json;

    #[tokio::test]
    async fn second_lookup_is_a_cache_hit() {
        let inner = MockD365Client::new(2, json!({ "legal_entity": "USMF" }));
        let cache = MetadataCache::new(inner, Duration::from_secs(60));
        let _ = cache
            .service_metadata("LedgerGeneralJournalEntryPost", "en-us")
            .await
            .unwrap();
        let _ = cache
            .service_metadata("LedgerGeneralJournalEntryPost", "en-us")
            .await
            .unwrap();
        let s = cache.stats().await;
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 1);
        assert_eq!(s.entries, 1);
    }

    #[tokio::test]
    async fn invalidate_clears_entries() {
        let inner = MockD365Client::new(2, json!({ "legal_entity": "USMF" }));
        let cache = MetadataCache::new(inner, Duration::from_secs(60));
        let _ = cache
            .service_metadata("EnvironmentInfo", "en-us")
            .await
            .unwrap();
        cache.invalidate_all().await;
        assert_eq!(cache.stats().await.entries, 0);
    }
}
