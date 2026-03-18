//! Semantic response cache for LLM queries.
//!
//! Caches text-only LLM responses so that identical or semantically similar
//! questions can be answered without calling the LLM provider at all.
//!
//! Similarity is determined by keyword overlap using Jaccard similarity.
//! Queries are normalized to lowercase keywords with common stop words
//! removed.  A configurable threshold (default 0.7) controls the minimum
//! similarity required for a cache hit.
//!
//! The cache enforces a maximum entry count and a per-entry TTL.  Expired
//! entries are evicted lazily (on access) or eagerly via [`evict_expired`].
//!
//! # What is **not** cached
//!
//! - Responses that involved tool calls (dynamic results).
//! - Very long responses (> 2000 chars — likely unique, not worth caching).
//! - Messages that start with `/` (slash commands).

use std::collections::HashMap;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum response length (in characters) that will be cached.
///
/// Responses longer than this are assumed to be unique / contextual and
/// are not worth storing.
pub const MAX_CACHEABLE_LENGTH: usize = 2000;

/// Stop words removed during keyword extraction.
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "can", "shall", "it", "its", "this", "that",
    "these", "those", "i", "me", "my", "we", "our", "you", "your", "he",
    "she", "they", "them", "their", "what", "which", "who", "whom", "how",
    "when", "where", "why", "of", "in", "to", "for", "on", "at", "by",
    "with", "from", "as", "into", "about", "and", "or", "but", "not", "no",
    "so", "if", "then",
];

// ---------------------------------------------------------------------------
// CacheEntry
// ---------------------------------------------------------------------------

/// A single cached response together with its metadata.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached response text.
    response: String,
    /// Keywords extracted from the original query.
    keywords: Vec<String>,
    /// When the entry was created.
    created_at: Instant,
    /// Number of times this entry has been served as a cache hit.
    hit_count: u32,
}

// ---------------------------------------------------------------------------
// CacheStats
// ---------------------------------------------------------------------------

/// Aggregate statistics for the semantic cache.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current number of entries in the cache.
    pub entries: usize,
    /// Total number of cache hits served since creation.
    pub total_hits: u32,
    /// Total number of cache misses since creation.
    pub total_misses: u32,
}

// ---------------------------------------------------------------------------
// SemanticCache
// ---------------------------------------------------------------------------

/// Keyword-based semantic response cache.
///
/// See the [module-level documentation](self) for details.
pub struct SemanticCache {
    /// Exact cache key (normalized query) → entry.
    entries: HashMap<String, CacheEntry>,
    /// Keyword → list of cache keys that contain that keyword.
    keyword_index: HashMap<String, Vec<String>>,
    /// Time-to-live for each cache entry.
    ttl: Duration,
    /// Maximum number of entries before the oldest are evicted.
    max_entries: usize,
    /// Cumulative hit counter.
    total_hits: u32,
    /// Cumulative miss counter.
    total_misses: u32,
}

/// Minimum Jaccard similarity to consider a cache hit.
const SIMILARITY_THRESHOLD: f64 = 0.7;

impl SemanticCache {
    /// Create a new cache with the given TTL and capacity.
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            keyword_index: HashMap::new(),
            ttl,
            max_entries,
            total_hits: 0,
            total_misses: 0,
        }
    }

    /// Try to find a cached response for the given query.
    ///
    /// Uses keyword overlap scoring (Jaccard similarity) to find semantic
    /// matches.  Returns `Some(response)` on a hit and `None` on a miss.
    pub fn get(&mut self, query: &str) -> Option<String> {
        // Lazily clean expired entries before searching.
        self.evict_expired();

        let query_keywords = Self::extract_keywords(query);
        if query_keywords.is_empty() {
            self.total_misses += 1;
            return None;
        }

        // Exact match first (fast path).
        let cache_key = Self::cache_key(query);
        if let Some(entry) = self.entries.get_mut(&cache_key) {
            if entry.created_at.elapsed() < self.ttl {
                entry.hit_count += 1;
                self.total_hits += 1;
                return Some(entry.response.clone());
            }
        }

        // Semantic match: gather candidate keys via the keyword index.
        let mut candidate_keys: HashMap<String, usize> = HashMap::new();
        for kw in &query_keywords {
            if let Some(keys) = self.keyword_index.get(kw) {
                for k in keys {
                    *candidate_keys.entry(k.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut best_key: Option<String> = None;
        let mut best_sim: f64 = 0.0;

        for (key, _count) in &candidate_keys {
            if let Some(entry) = self.entries.get(key) {
                if entry.created_at.elapsed() >= self.ttl {
                    continue; // expired
                }
                let sim = Self::similarity(&query_keywords, &entry.keywords);
                if sim >= SIMILARITY_THRESHOLD && sim > best_sim {
                    best_sim = sim;
                    best_key = Some(key.clone());
                }
            }
        }

        if let Some(key) = best_key {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.hit_count += 1;
                self.total_hits += 1;
                return Some(entry.response.clone());
            }
        }

        self.total_misses += 1;
        None
    }

    /// Store a response in the cache.
    ///
    /// If the cache is full, the oldest entry is evicted first.
    pub fn put(&mut self, query: &str, response: &str) {
        // Don't cache very long responses.
        if response.len() > MAX_CACHEABLE_LENGTH {
            return;
        }

        // Don't cache slash commands.
        if query.starts_with('/') {
            return;
        }

        // Evict if we're at capacity.
        if self.entries.len() >= self.max_entries {
            self.evict_oldest();
        }

        let keywords = Self::extract_keywords(query);
        let cache_key = Self::cache_key(query);

        // Update the keyword index.
        for kw in &keywords {
            self.keyword_index
                .entry(kw.clone())
                .or_default()
                .push(cache_key.clone());
        }

        self.entries.insert(
            cache_key,
            CacheEntry {
                response: response.to_string(),
                keywords,
                created_at: Instant::now(),
                hit_count: 0,
            },
        );
    }

    /// Extract keywords from a text.
    ///
    /// Lowercases the text, splits on non-alphanumeric characters, removes
    /// stop words, and removes very short tokens (< 2 chars).
    fn extract_keywords(text: &str) -> Vec<String> {
        let lower = text.to_lowercase();
        lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .filter(|w| w.len() >= 2)
            .filter(|w| !STOP_WORDS.contains(w))
            .map(|w| w.to_string())
            .collect()
    }

    /// Calculate Jaccard similarity between two keyword sets.
    ///
    /// Returns a value in `[0.0, 1.0]`.
    fn similarity(a: &[String], b: &[String]) -> f64 {
        if a.is_empty() && b.is_empty() {
            return 1.0;
        }
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        let set_a: std::collections::HashSet<&str> =
            a.iter().map(|s| s.as_str()).collect();
        let set_b: std::collections::HashSet<&str> =
            b.iter().map(|s| s.as_str()).collect();

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Remove all expired entries from the cache.
    pub fn evict_expired(&mut self) {
        let ttl = self.ttl;
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.created_at.elapsed() >= ttl)
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            self.remove_entry(&key);
        }
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.keyword_index.clear();
    }

    /// Return the number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return aggregate cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            total_hits: self.total_hits,
            total_misses: self.total_misses,
        }
    }

    // -- Internal helpers -----------------------------------------------------

    /// Produce a normalized cache key from a query string.
    fn cache_key(query: &str) -> String {
        query.to_lowercase().trim().to_string()
    }

    /// Evict the oldest entry to make room for a new one.
    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(key, _)| key.clone())
        {
            self.remove_entry(&oldest_key);
        }
    }

    /// Remove a single entry and clean up its keyword index references.
    fn remove_entry(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            // Clean up keyword index.
            for kw in &entry.keywords {
                if let Some(keys) = self.keyword_index.get_mut(kw) {
                    keys.retain(|k| k != key);
                    if keys.is_empty() {
                        self.keyword_index.remove(kw);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: cache with generous TTL and capacity.
    fn test_cache() -> SemanticCache {
        SemanticCache::new(Duration::from_secs(300), 100)
    }

    // -- Keyword extraction ---------------------------------------------------

    #[test]
    fn extract_keywords_basic() {
        let kws = SemanticCache::extract_keywords("What is the disk usage?");
        assert!(kws.contains(&"disk".to_string()));
        assert!(kws.contains(&"usage".to_string()));
        // "what", "is", "the" are stop words.
        assert!(!kws.contains(&"what".to_string()));
        assert!(!kws.contains(&"is".to_string()));
        assert!(!kws.contains(&"the".to_string()));
    }

    #[test]
    fn extract_keywords_removes_short_tokens() {
        let kws = SemanticCache::extract_keywords("I am a dev");
        // "i", "a" are stop words and/or < 2 chars.
        assert!(!kws.contains(&"i".to_string()));
        assert!(!kws.contains(&"a".to_string()));
        // "am" is 2 chars but is not a stop word — included.
        assert!(kws.contains(&"am".to_string()));
        assert!(kws.contains(&"dev".to_string()));
    }

    #[test]
    fn extract_keywords_empty_input() {
        let kws = SemanticCache::extract_keywords("");
        assert!(kws.is_empty());
    }

    #[test]
    fn extract_keywords_only_stop_words() {
        let kws = SemanticCache::extract_keywords("the is a an");
        assert!(kws.is_empty());
    }

    #[test]
    fn extract_keywords_lowercases() {
        let kws = SemanticCache::extract_keywords("DISK USAGE CHECK");
        assert!(kws.contains(&"disk".to_string()));
        assert!(kws.contains(&"usage".to_string()));
        assert!(kws.contains(&"check".to_string()));
    }

    #[test]
    fn extract_keywords_handles_punctuation() {
        let kws = SemanticCache::extract_keywords("Hello, world! How's it going?");
        assert!(kws.contains(&"hello".to_string()));
        assert!(kws.contains(&"world".to_string()));
        assert!(kws.contains(&"going".to_string()));
    }

    // -- Jaccard similarity ---------------------------------------------------

    #[test]
    fn similarity_identical_sets() {
        let a = vec!["disk".into(), "usage".into()];
        let b = vec!["disk".into(), "usage".into()];
        assert!((SemanticCache::similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_disjoint_sets() {
        let a = vec!["disk".into(), "usage".into()];
        let b = vec!["network".into(), "speed".into()];
        assert!((SemanticCache::similarity(&a, &b)).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_partial_overlap() {
        // {disk, usage, check} ∩ {disk, usage, report} = {disk, usage}
        // |intersection| = 2, |union| = 4, similarity = 0.5
        let a = vec!["disk".into(), "usage".into(), "check".into()];
        let b = vec!["disk".into(), "usage".into(), "report".into()];
        let sim = SemanticCache::similarity(&a, &b);
        assert!((sim - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_both_empty() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert!((SemanticCache::similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_one_empty() {
        let a = vec!["disk".into()];
        let b: Vec<String> = vec![];
        assert!((SemanticCache::similarity(&a, &b)).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_high_overlap() {
        // {disk, usage} ∩ {disk, usage, report} = {disk, usage}
        // |intersection| = 2, |union| = 3, sim ≈ 0.667
        let a = vec!["disk".into(), "usage".into()];
        let b = vec!["disk".into(), "usage".into(), "report".into()];
        let sim = SemanticCache::similarity(&a, &b);
        assert!(sim > 0.6 && sim < 0.7);
    }

    // -- Cache put / get (exact match) ----------------------------------------

    #[test]
    fn put_and_get_exact_match() {
        let mut cache = test_cache();
        cache.put("What is the disk usage?", "Disk is 42% full.");
        let result = cache.get("What is the disk usage?");
        assert_eq!(result, Some("Disk is 42% full.".to_string()));
    }

    #[test]
    fn get_miss_returns_none() {
        let mut cache = test_cache();
        let result = cache.get("Something completely unrelated");
        assert_eq!(result, None);
    }

    #[test]
    fn case_insensitive_exact_match() {
        let mut cache = test_cache();
        cache.put("what is the disk usage?", "42% full.");
        let result = cache.get("What Is The Disk Usage?");
        assert_eq!(result, Some("42% full.".to_string()));
    }

    // -- Cache semantic matching -----------------------------------------------

    #[test]
    fn semantic_match_similar_queries() {
        let mut cache = test_cache();
        cache.put("disk usage check", "42% used.");
        // "check disk usage" has the same keywords.
        let result = cache.get("check disk usage");
        assert_eq!(result, Some("42% used.".to_string()));
    }

    #[test]
    fn no_match_for_different_queries() {
        let mut cache = test_cache();
        cache.put("disk usage check", "42% used.");
        // Completely different topic.
        let result = cache.get("network bandwidth test");
        assert_eq!(result, None);
    }

    // -- TTL expiry -----------------------------------------------------------

    #[test]
    fn expired_entries_are_not_returned() {
        let mut cache = SemanticCache::new(Duration::from_millis(1), 100);
        cache.put("test query here", "cached response");
        // Sleep just past the TTL.
        std::thread::sleep(Duration::from_millis(10));
        let result = cache.get("test query here");
        assert_eq!(result, None);
    }

    #[test]
    fn evict_expired_removes_old_entries() {
        let mut cache = SemanticCache::new(Duration::from_millis(1), 100);
        cache.put("query one test", "response one");
        cache.put("query two test", "response two");
        assert_eq!(cache.len(), 2);

        std::thread::sleep(Duration::from_millis(10));
        cache.evict_expired();
        assert_eq!(cache.len(), 0);
    }

    // -- Capacity / eviction --------------------------------------------------

    #[test]
    fn evict_oldest_when_full() {
        let mut cache = SemanticCache::new(Duration::from_secs(300), 2);
        cache.put("first query here", "response 1");
        // Small delay to ensure different creation times.
        std::thread::sleep(Duration::from_millis(5));
        cache.put("second query here", "response 2");
        assert_eq!(cache.len(), 2);

        // Adding a third should evict the first.
        cache.put("third query here", "response 3");
        assert_eq!(cache.len(), 2);

        // First should be gone.
        assert!(cache.get("first query here").is_none());
        // Second and third should remain.
        assert!(cache.get("second query here").is_some());
        assert!(cache.get("third query here").is_some());
    }

    // -- Don't cache slash commands -------------------------------------------

    #[test]
    fn slash_commands_not_cached() {
        let mut cache = test_cache();
        cache.put("/help", "Available commands...");
        assert_eq!(cache.len(), 0);
    }

    // -- Don't cache very long responses --------------------------------------

    #[test]
    fn long_responses_not_cached() {
        let mut cache = test_cache();
        let long_response = "x".repeat(MAX_CACHEABLE_LENGTH + 1);
        cache.put("some query here", &long_response);
        assert_eq!(cache.len(), 0);
    }

    // -- Clear ----------------------------------------------------------------

    #[test]
    fn clear_empties_cache() {
        let mut cache = test_cache();
        cache.put("query one here", "response 1");
        cache.put("query two here", "response 2");
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    // -- Stats ----------------------------------------------------------------

    #[test]
    fn stats_track_hits_and_misses() {
        let mut cache = test_cache();
        cache.put("disk usage check", "42%");

        // Hit.
        let _ = cache.get("disk usage check");
        // Miss.
        let _ = cache.get("network bandwidth test");
        // Hit.
        let _ = cache.get("check disk usage");

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.total_hits, 2);
        assert_eq!(stats.total_misses, 1);
    }

    // -- is_empty / len -------------------------------------------------------

    #[test]
    fn is_empty_on_new_cache() {
        let cache = test_cache();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn len_after_inserts() {
        let mut cache = test_cache();
        cache.put("query alpha test", "alpha");
        cache.put("query beta test", "beta");
        assert_eq!(cache.len(), 2);
        assert!(!cache.is_empty());
    }

    // -- Edge cases -----------------------------------------------------------

    #[test]
    fn empty_query_returns_miss() {
        let mut cache = test_cache();
        cache.put("disk usage check", "42%");
        assert_eq!(cache.get(""), None);
    }

    #[test]
    fn keyword_index_cleaned_on_eviction() {
        let mut cache = SemanticCache::new(Duration::from_millis(1), 100);
        cache.put("disk usage check", "42%");
        assert!(!cache.keyword_index.is_empty());

        std::thread::sleep(Duration::from_millis(10));
        cache.evict_expired();
        assert!(cache.keyword_index.is_empty());
    }

    #[test]
    fn duplicate_put_overwrites() {
        let mut cache = test_cache();
        cache.put("disk usage check", "42%");
        cache.put("disk usage check", "50%");
        // The latest value should be returned.
        let result = cache.get("disk usage check");
        assert_eq!(result, Some("50%".to_string()));
    }

    // -- CacheStats debug -----------------------------------------------------

    #[test]
    fn cache_stats_debug() {
        let stats = CacheStats {
            entries: 5,
            total_hits: 10,
            total_misses: 3,
        };
        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("entries: 5"));
        assert!(debug_str.contains("total_hits: 10"));
        assert!(debug_str.contains("total_misses: 3"));
    }

    // -- Additional tests --

    #[test]
    fn cache_miss_returns_none() {
        let mut cache = test_cache();
        // Empty cache should always return None.
        assert_eq!(cache.get("any query at all"), None);
        assert_eq!(cache.get("disk usage"), None);
        assert_eq!(cache.get("network speed"), None);

        // After adding one entry, unrelated queries should still miss.
        cache.put("disk usage check", "42%");
        assert_eq!(cache.get("install python packages"), None);
    }

    #[test]
    fn cache_hit_returns_stored() {
        let mut cache = test_cache();

        // Exact match hit.
        cache.put("disk usage check", "42% full");
        assert_eq!(cache.get("disk usage check"), Some("42% full".to_string()));

        // Multiple entries, each should be retrievable.
        cache.put("network speed test", "100 Mbps");
        cache.put("memory usage check", "8 GB used");
        assert_eq!(cache.get("network speed test"), Some("100 Mbps".to_string()));
        assert_eq!(cache.get("memory usage check"), Some("8 GB used".to_string()));
        // Original entry should still be there.
        assert_eq!(cache.get("disk usage check"), Some("42% full".to_string()));
    }

    #[test]
    fn cache_expires_after_ttl() {
        // Create a cache with very short TTL.
        let mut cache = SemanticCache::new(Duration::from_millis(50), 100);
        cache.put("test query here", "cached result");

        // Should hit immediately.
        assert_eq!(cache.get("test query here"), Some("cached result".to_string()));

        // Wait for TTL to expire.
        std::thread::sleep(Duration::from_millis(100));

        // Should miss after expiry.
        assert_eq!(cache.get("test query here"), None);

        // Verify the entry was actually evicted.
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn similar_queries_hit_cache() {
        let mut cache = test_cache();

        // Store a response.
        cache.put("disk usage check", "42% used");

        // Query with same keywords in different order should hit.
        assert_eq!(cache.get("check disk usage"), Some("42% used".to_string()));

        // Query with same keywords plus stop words should hit.
        assert_eq!(
            cache.get("what is the disk usage check"),
            Some("42% used".to_string()),
        );

        // Query with slightly different but overlapping keywords.
        // "disk" + "usage" overlap with "disk" + "usage" + "check" = Jaccard 2/3 ≈ 0.67
        // This is below the 0.7 threshold, so it may miss depending on exact keywords.
        // Let's test a clear hit case instead.
        cache.put("system memory usage report", "8 GB free");
        assert_eq!(
            cache.get("memory usage system report"),
            Some("8 GB free".to_string()),
        );
    }

    #[test]
    fn cache_respects_max_entries() {
        let mut cache = SemanticCache::new(Duration::from_secs(300), 3);

        cache.put("query alpha test", "response alpha");
        std::thread::sleep(Duration::from_millis(5));
        cache.put("query beta test", "response beta");
        std::thread::sleep(Duration::from_millis(5));
        cache.put("query gamma test", "response gamma");
        assert_eq!(cache.len(), 3);

        // Adding a 4th should evict the oldest (alpha).
        cache.put("query delta test", "response delta");
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("query alpha test"), None);
        assert_eq!(cache.get("query beta test"), Some("response beta".to_string()));
        assert_eq!(cache.get("query gamma test"), Some("response gamma".to_string()));
        assert_eq!(cache.get("query delta test"), Some("response delta".to_string()));

        // Adding a 5th should evict beta (now the oldest).
        cache.put("query epsilon test", "response epsilon");
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("query beta test"), None);
    }

    #[test]
    fn slash_commands_skip_cache() {
        let mut cache = test_cache();

        // Slash commands should not be stored.
        cache.put("/help", "Available commands...");
        assert_eq!(cache.len(), 0);

        cache.put("/provider claude", "Switched to Claude");
        assert_eq!(cache.len(), 0);

        cache.put("/model gpt-4", "Model set to gpt-4");
        assert_eq!(cache.len(), 0);

        // Non-slash commands should be stored.
        cache.put("what time is it", "3pm");
        assert_eq!(cache.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Cache hit for identical query — verify response text and hit counters
    // -----------------------------------------------------------------------

    #[test]
    fn cache_hit_identical_query_multiple_times() {
        let mut cache = test_cache();
        cache.put("What is the disk usage?", "Disk is 42% full.");

        // Three identical lookups.
        for _ in 0..3 {
            assert_eq!(
                cache.get("What is the disk usage?"),
                Some("Disk is 42% full.".to_string()),
            );
        }
        let stats = cache.stats();
        assert_eq!(stats.total_hits, 3);
        assert_eq!(stats.total_misses, 0);
    }

    // -----------------------------------------------------------------------
    // Cache hit for similar query via Jaccard similarity
    // -----------------------------------------------------------------------

    #[test]
    fn cache_hit_similar_query_keyword_reorder() {
        let mut cache = test_cache();
        cache.put("memory usage check", "Memory at 60%.");
        // Same keywords in different order — Jaccard = 1.0.
        let result = cache.get("check memory usage");
        assert_eq!(result, Some("Memory at 60%.".to_string()));
    }

    // -----------------------------------------------------------------------
    // Cache miss for different query — no keyword overlap
    // -----------------------------------------------------------------------

    #[test]
    fn cache_miss_zero_overlap() {
        let mut cache = test_cache();
        cache.put("disk usage check", "42%");
        // No shared keywords.
        assert_eq!(cache.get("install python library packages"), None);
        let stats = cache.stats();
        assert_eq!(stats.total_misses, 1);
    }

    // -----------------------------------------------------------------------
    // Cache TTL: recently inserted entry is valid, expired is not
    // -----------------------------------------------------------------------

    #[test]
    fn cache_ttl_fresh_entry_returned() {
        let mut cache = SemanticCache::new(Duration::from_secs(60), 100);
        cache.put("quick query check", "fast response");
        // Immediately available.
        assert_eq!(
            cache.get("quick query check"),
            Some("fast response".to_string()),
        );
    }

    #[test]
    fn cache_ttl_expired_semantic_match_not_returned() {
        let mut cache = SemanticCache::new(Duration::from_millis(1), 100);
        cache.put("disk usage check", "42%");
        std::thread::sleep(Duration::from_millis(10));
        // Semantic match with identical keywords should still miss.
        assert_eq!(cache.get("check disk usage"), None);
    }

    // -----------------------------------------------------------------------
    // Max-size eviction — single-entry cache
    // -----------------------------------------------------------------------

    #[test]
    fn cache_eviction_max_size_one() {
        let mut cache = SemanticCache::new(Duration::from_secs(300), 1);
        cache.put("first query here", "response 1");
        assert_eq!(cache.len(), 1);
        cache.put("second query here", "response 2");
        assert_eq!(cache.len(), 1);
        assert!(cache.get("first query here").is_none());
        assert_eq!(
            cache.get("second query here"),
            Some("response 2".to_string()),
        );
    }

    // -----------------------------------------------------------------------
    // Skip long responses — boundary test
    // -----------------------------------------------------------------------

    #[test]
    fn cache_skip_long_response_exactly_at_limit_is_cached() {
        let mut cache = test_cache();
        let response = "x".repeat(MAX_CACHEABLE_LENGTH);
        cache.put("boundary query here", &response);
        // Exactly at limit — should be cached.
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_skip_long_response_one_over_limit_is_not_cached() {
        let mut cache = test_cache();
        let response = "x".repeat(MAX_CACHEABLE_LENGTH + 1);
        cache.put("boundary query here", &response);
        assert_eq!(cache.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Skip slash commands (tool-call triggers)
    // -----------------------------------------------------------------------

    #[test]
    fn cache_skip_all_slash_variants() {
        let mut cache = test_cache();
        cache.put("/key claude sk-ant-123", "Key set.");
        cache.put("/effort high", "Effort set to high.");
        cache.put("/clear", "Chat cleared.");
        assert_eq!(cache.len(), 0);
    }
}
