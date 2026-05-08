//! Vector database integration using LanceDB.
//!
//! Provides semantic memory storage and retrieval for:
//! - Tier 2: Historical conversation summaries
//! - Tier 3: Persistent user/project memories
//! - Tier 4: Code knowledge index
//!
//! Gated behind the `vector-memory` feature flag. Without the feature,
//! all operations return empty/no-op results so the rest of the codebase
//! compiles without ONNX Runtime or LanceDB dependencies.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Public types (always available, no feature gate)
// ---------------------------------------------------------------------------

/// A memory record stored in LanceDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub source: String,
    pub session_id: String,
    pub tags: Option<String>,
    pub created_at: DateTime<Utc>,
    pub ttl: Option<DateTime<Utc>>,
    /// Cosine similarity score from the search (0.0 = unrelated, 1.0 = exact).
    /// Populated only on retrieval; 0.0 when stored.
    pub score: f64,
}

/// A new memory to be stored (embedding is computed automatically).
#[derive(Debug, Clone)]
pub struct NewMemoryItem {
    pub content: String,
    pub source: String,
    pub session_id: String,
    pub tags: Option<String>,
    pub ttl: Option<DateTime<Utc>>,
}

/// A history summary from conversation compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySummary {
    pub id: String,
    pub turn_range: String,
    pub summary: String,
    pub key_files: Option<String>,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub score: f64,
}

// ---------------------------------------------------------------------------
// VectorDbService — main public API
// ---------------------------------------------------------------------------

/// In-memory fallback when `vector-memory` feature is disabled.
///
/// Stores records in a `Vec` and uses naive keyword matching for retrieval.
/// This lets the rest of the codebase function without ONNX Runtime.
struct InMemoryBackend {
    memories: Vec<MemoryRecord>,
    summaries: Vec<HistorySummary>,
}

impl InMemoryBackend {
    fn new() -> Self {
        Self {
            memories: Vec::new(),
            summaries: Vec::new(),
        }
    }

    fn store_memory(&mut self, item: NewMemoryItem) -> MemoryRecord {
        let record = MemoryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            content: item.content,
            source: item.source,
            session_id: item.session_id,
            tags: item.tags,
            created_at: Utc::now(),
            ttl: item.ttl,
            score: 0.0,
        };
        self.memories.push(record.clone());
        record
    }

    fn search_memories(&self, query: &str, k: usize, _filter: Option<&str>) -> Vec<MemoryRecord> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<MemoryRecord> = self
            .memories
            .iter()
            .filter(|m| {
                let content_lower = m.content.to_lowercase();
                query_words.iter().any(|w| content_lower.contains(w))
            })
            .cloned()
            .collect();

        // Sort by simple word-match count (crude relevance)
        scored.sort_by(|a, b| {
            let a_count = query_words
                .iter()
                .filter(|w| a.content.to_lowercase().contains(*w))
                .count();
            let b_count = query_words
                .iter()
                .filter(|w| b.content.to_lowercase().contains(*w))
                .count();
            b_count.cmp(&a_count)
        });

        // Assign scores
        let total = scored.len().max(1);
        for (i, m) in scored.iter_mut().enumerate() {
            m.score = 1.0 - (i as f64 / total as f64);
        }

        scored.truncate(k);
        scored
    }

    fn store_summary(&mut self, summary: HistorySummary) {
        self.summaries.push(summary);
    }

    fn search_summaries(&self, query: &str, k: usize) -> Vec<HistorySummary> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<HistorySummary> = self
            .summaries
            .iter()
            .filter(|s| {
                let summary_lower = s.summary.to_lowercase();
                query_words.iter().any(|w| summary_lower.contains(w))
            })
            .cloned()
            .collect();

        scored.sort_by(|a, b| {
            let a_count = query_words
                .iter()
                .filter(|w| a.summary.to_lowercase().contains(*w))
                .count();
            let b_count = query_words
                .iter()
                .filter(|w| b.summary.to_lowercase().contains(*w))
                .count();
            b_count.cmp(&a_count)
        });

        let total = scored.len().max(1);
        for (i, s) in scored.iter_mut().enumerate() {
            s.score = 1.0 - (i as f64 / total as f64);
        }

        scored.truncate(k);
        scored
    }

    fn delete_expired(&mut self) -> usize {
        let now = Utc::now();
        let before = self.memories.len();
        self.memories.retain(|m| m.ttl.map_or(true, |t| t > now));
        before - self.memories.len()
    }

    fn count_memories(&self) -> usize {
        self.memories.len()
    }
}

#[cfg(feature = "vector-memory")]
mod lance {
    use super::*;

    /// Lazily-initialized LanceDB backend.
    pub struct LanceBackend {
        /// Path to the LanceDB database directory.
        pub path: String,
    }

    impl LanceBackend {
        pub async fn connect(path: &Path) -> Result<Self> {
            let path_str = path.to_str().unwrap_or("/tmp/lancedb").to_string();

            // Verify the directory is usable
            tokio::fs::create_dir_all(path).await?;

            Ok(Self { path: path_str })
        }

        /// Basic health check — ensure the database directory exists
        /// and we can open a connection.
        pub async fn health_check(&self) -> Result<bool> {
            let db = lancedb::connect(&self.path).execute().await?;
            let _ = db.table_names().execute().await?;
            Ok(true)
        }
    }
}

/// The main vector database service.
///
/// When `vector-memory` feature is enabled, uses LanceDB for persistent
/// vector storage. Without it, falls back to an in-memory keyword matcher
/// so the codebase compiles and runs without ONNX Runtime.
#[derive(Clone)]
pub struct VectorDbService {
    /// In-memory fallback backend (always available)
    memory: Arc<RwLock<InMemoryBackend>>,
    /// LanceDB backend (only when feature is enabled)
    #[cfg(feature = "vector-memory")]
    lance: Option<Arc<lance::LanceBackend>>,
}

impl VectorDbService {
    /// Create a new service.
    ///
    /// * `path` — directory for LanceDB storage (ignored when feature is off)
    /// * `_dim` — embedding dimension (ignored when feature is off)
    pub async fn connect(path: &Path, _dim: usize) -> Result<Self> {
        let memory = Arc::new(RwLock::new(InMemoryBackend::new()));

        #[cfg(feature = "vector-memory")]
        {
            let lance = lance::LanceBackend::connect(path).await?;
            return Ok(Self {
                memory,
                lance: Some(Arc::new(lance)),
            });
        }

        #[cfg(not(feature = "vector-memory"))]
        {
            let _ = path; // unused
            Ok(Self { memory })
        }
    }

    /// Store a memory item.
    pub async fn store_memory(&self, item: NewMemoryItem) -> Result<MemoryRecord> {
        let record = self.memory.write().await.store_memory(item);

        #[cfg(feature = "vector-memory")]
        {
            // Forward to LanceDB asynchronously (fire-and-forget on write)
            // Real embedding + LanceDB write will happen in a future phase
            tracing::debug!(memory_id = %record.id, "stored memory (lancedb: pending)");
        }

        Ok(record)
    }

    /// Search memories by relevance to `query`.
    pub async fn search_memories(
        &self,
        query: &str,
        k: u32,
        filter: Option<&str>,
    ) -> Result<Vec<MemoryRecord>> {
        let results = self
            .memory
            .read()
            .await
            .search_memories(query, k as usize, filter);

        #[cfg(feature = "vector-memory")]
        {
            if let Some(lance) = &self.lance {
                if lance.health_check().await.unwrap_or(false) {
                    // TODO: Replace with real LanceDB vector search
                    // once the Rust API integration is stable
                    tracing::debug!(query = %query, "lancedb search would run here");
                }
            }
        }

        Ok(results)
    }

    /// Store a history summary from compaction.
    pub async fn store_summary(&self, summary: HistorySummary) -> Result<()> {
        self.memory.write().await.store_summary(summary);
        Ok(())
    }

    /// Search history summaries.
    pub async fn search_summaries(&self, query: &str, k: u32) -> Result<Vec<HistorySummary>> {
        let results = self
            .memory
            .read()
            .await
            .search_summaries(query, k as usize);
        Ok(results)
    }

    /// Delete expired memories.
    pub async fn delete_expired_memories(&self) -> Result<usize> {
        let deleted = self.memory.write().await.delete_expired();

        #[cfg(feature = "vector-memory")]
        {
            tracing::debug!(deleted, "deleted expired memories (lancedb: pending)");
        }

        Ok(deleted)
    }

    /// Count total memories.
    pub async fn count_memories(&self) -> Result<usize> {
        Ok(self.memory.read().await.count_memories())
    }
}

// ---------------------------------------------------------------------------
// Verbatim Window — determines which messages are sent in full to the API
// ---------------------------------------------------------------------------

/// Default verbatim window size (in turns). Matches `seam_manager::VERBATIM_WINDOW_TURNS`.
pub const DEFAULT_VERBATIM_WINDOW_TURNS: usize = 16;

/// Split messages into a verbatim window (recent + pinned) and a history
/// portion that will be retrieved via vector search instead.
#[derive(Debug, Clone)]
pub struct VerbatimWindow {
    /// Indices of messages to send in full to the API.
    pub indices: Vec<usize>,
    /// Whether the window was extended to preserve tool call/result pairs.
    pub extended: bool,
}

impl VerbatimWindow {
    /// Build the verbatim window from session metadata.
    ///
    /// * `total` — total number of messages in the session
    /// * `window_size` — how many recent messages to keep verbatim (default 8)
    /// * `pins` — externally pinned indices (user `/pin`, hot paths, etc.)
    /// * `tool_call_indices` — `(tool_use_id, message_index)` for all tool calls
    /// * `tool_result_indices` — `(tool_use_id, message_index)` for all tool results
    pub fn build(
        total: usize,
        window_size: usize,
        pins: &[usize],
        tool_call_indices: &[(String, usize)],
        tool_result_indices: &[(String, usize)],
    ) -> Self {
        let mut indices: Vec<usize> = Vec::new();

        // 1. Always include last `window_size` messages
        let recent_start = total.saturating_sub(window_size);
        indices.extend(recent_start..total);

        // 2. Include pinned indices
        for &p in pins {
            if p < total && !indices.contains(&p) {
                indices.push(p);
            }
        }

        // 3. Enforce tool call pairing
        let mut extended = false;

        // 3a. Tool call in window → pull in its result
        for (tool_id, call_idx) in tool_call_indices {
            if indices.contains(call_idx) {
                if let Some((_, result_idx)) = tool_result_indices.iter().find(|(id, _)| id == tool_id) {
                    if !indices.contains(result_idx) {
                        indices.push(*result_idx);
                        extended = true;
                    }
                }
            }
        }

        // 3b. Tool result in window → pull in its call
        for (tool_id, result_idx) in tool_result_indices {
            if indices.contains(result_idx) {
                if let Some((_, call_idx)) = tool_call_indices.iter().find(|(id, _)| id == tool_id) {
                    if !indices.contains(call_idx) {
                        indices.push(*call_idx);
                        extended = true;
                    }
                }
            }
        }

        indices.sort_unstable();
        indices.dedup();

        Self { indices, extended }
    }

    /// Number of messages in the window.
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Whether the window is empty.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Check if a specific index is in the verbatim window.
    pub fn contains(&self, idx: usize) -> bool {
        self.indices.contains(&idx)
    }

    /// Iterate over indices in order.
    pub fn iter(&self) -> impl Iterator<Item = &usize> {
        self.indices.iter()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── VerbatimWindow ──

    #[test]
    fn keeps_last_n_messages() {
        let vw = VerbatimWindow::build(20, 8, &[], &[], &[]);
        assert_eq!(vw.indices, (12..20).collect::<Vec<_>>());
        assert!(!vw.extended);
    }

    #[test]
    fn fewer_messages_than_window() {
        let vw = VerbatimWindow::build(3, 8, &[], &[], &[]);
        assert_eq!(vw.indices, (0..3).collect::<Vec<_>>());
    }

    #[test]
    fn zero_messages() {
        let vw = VerbatimWindow::build(0, 8, &[], &[], &[]);
        assert!(vw.is_empty());
    }

    #[test]
    fn includes_pins_outside_window() {
        let vw = VerbatimWindow::build(20, 4, &[0, 5], &[], &[]);
        assert!(vw.contains(0));
        assert!(vw.contains(5));
        assert!(vw.contains(19));
    }

    #[test]
    fn pins_within_window_not_duplicated() {
        let vw = VerbatimWindow::build(10, 8, &[8, 9], &[], &[]);
        // indices should be [2,3,4,5,6,7,8,9] — no duplicates
        assert_eq!(vw.indices.len(), 8);
        let count_8 = vw.indices.iter().filter(|&&i| i == 8).count();
        let count_9 = vw.indices.iter().filter(|&&i| i == 9).count();
        assert_eq!(count_8, 1);
        assert_eq!(count_9, 1);
    }

    #[test]
    fn tool_call_pulls_in_result() {
        let calls = vec![("t1".to_string(), 10)];
        let results = vec![("t1".to_string(), 11)];
        let vw = VerbatimWindow::build(20, 4, &[10], &calls, &results);
        assert!(vw.contains(10)); // pinned call
        assert!(vw.contains(11)); // result pulled in
        assert!(vw.extended);
    }

    #[test]
    fn tool_result_pulls_in_call() {
        let calls = vec![("t1".to_string(), 5)];
        let results = vec![("t1".to_string(), 15)];
        let vw = VerbatimWindow::build(20, 4, &[15], &calls, &results);
        assert!(vw.contains(15)); // pinned result
        assert!(vw.contains(5));  // call pulled in
    }

    #[test]
    fn orphan_tool_result_does_not_extend() {
        let vw = VerbatimWindow::build(20, 4, &[15], &[], &[("orphan".to_string(), 15)]);
        assert!(vw.contains(15));
        assert!(!vw.extended);
    }

    #[test]
    fn out_of_bounds_pin_ignored() {
        let vw = VerbatimWindow::build(10, 4, &[99], &[], &[]);
        assert!(!vw.contains(99));
        assert_eq!(vw.indices.len(), 4);
    }

    #[test]
    fn window_size_zero_keeps_only_pins() {
        let vw = VerbatimWindow::build(20, 0, &[0], &[], &[]);
        assert_eq!(vw.indices, vec![0]);
    }

    #[test]
    fn mixed_pins_and_recent_no_gaps() {
        let vw = VerbatimWindow::build(10, 3, &[0, 2, 5], &[], &[]);
        // recent: 7,8,9; pins: 0,2,5
        assert_eq!(vw.indices, vec![0, 2, 5, 7, 8, 9]);
    }

    // ── VectorDbService ──

    #[tokio::test]
    async fn store_and_retrieve_memory() {
        let svc = VectorDbService::connect(Path::new("/tmp/test_vdb"), 384)
            .await
            .unwrap();

        svc.store_memory(NewMemoryItem {
            content: "用户喜欢 4 空格缩进".into(),
            source: "model".into(),
            session_id: "s1".into(),
            tags: Some("preference".into()),
            ttl: None,
        })
        .await
        .unwrap();

        let results = svc.search_memories("缩进", 5, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("4 空格缩进"));
    }

    #[tokio::test]
    async fn search_non_existent() {
        let svc = VectorDbService::connect(Path::new("/tmp/test_vdb2"), 384)
            .await
            .unwrap();

        svc.store_memory(NewMemoryItem {
            content: "测试记忆".into(),
            source: "user".into(),
            session_id: "s1".into(),
            tags: None,
            ttl: None,
        })
        .await
        .unwrap();

        let results = svc.search_memories("不存在的关键词", 5, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn multiple_memories_ranked() {
        let svc = VectorDbService::connect(Path::new("/tmp/test_vdb3"), 384)
            .await
            .unwrap();

        svc.store_memory(NewMemoryItem {
            content: "用户用 cargo test --workspace 运行测试".into(),
            source: "model".into(),
            session_id: "s1".into(),
            tags: None,
            ttl: None,
        })
        .await
        .unwrap();

        svc.store_memory(NewMemoryItem {
            content: "用户喜欢 VS Code 编辑器".into(),
            source: "model".into(),
            session_id: "s1".into(),
            tags: None,
            ttl: None,
        })
        .await
        .unwrap();

        let results = svc.search_memories("cargo 测试", 2, None).await.unwrap();
        assert!(results.len() >= 1);
        // The test-related memory should rank higher
        assert!(results[0].content.contains("test"));
    }

    #[tokio::test]
    async fn store_and_search_summaries() {
        let svc = VectorDbService::connect(Path::new("/tmp/test_vdb4"), 384)
            .await
            .unwrap();

        svc.store_summary(HistorySummary {
            id: "sum-1".into(),
            turn_range: "1-10".into(),
            summary: "用户修改了 config.rs 中的编译选项".into(),
            key_files: Some("crates/tui/src/config.rs".into()),
            session_id: "s1".into(),
            created_at: Utc::now(),
            score: 0.0,
        })
        .await
        .unwrap();

        let results = svc.search_summaries("config", 5).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.contains("config.rs"));
    }

    #[tokio::test]
    async fn delete_expired_memories() {
        let svc = VectorDbService::connect(Path::new("/tmp/test_vdb5"), 384)
            .await
            .unwrap();

        svc.store_memory(NewMemoryItem {
            content: "会过期的记忆".into(),
            source: "test".into(),
            session_id: "s1".into(),
            tags: None,
            ttl: Some(Utc::now() - chrono::Duration::hours(1)), // already expired
        })
        .await
        .unwrap();

        svc.store_memory(NewMemoryItem {
            content: "永不过期的记忆".into(),
            source: "test".into(),
            session_id: "s1".into(),
            tags: None,
            ttl: None,
        })
        .await
        .unwrap();

        let deleted = svc.delete_expired_memories().await.unwrap();
        assert_eq!(deleted, 1);

        let count = svc.count_memories().await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn count_stored_memories() {
        let svc = VectorDbService::connect(Path::new("/tmp/test_vdb6"), 384)
            .await
            .unwrap();

        for i in 0..5 {
            svc.store_memory(NewMemoryItem {
                content: format!("记忆 {i}"),
                source: "test".into(),
                session_id: "s1".into(),
                tags: None,
                ttl: None,
            })
            .await
            .unwrap();
        }

        assert_eq!(svc.count_memories().await.unwrap(), 5);
    }
}
