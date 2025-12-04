use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

const CHECKPOINT_DIR: &str = ".checkpoint";
const CHECKPOINT_FILE: &str = "progress.json";

/// Checkpoint data for resume functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Table being processed
    pub table: String,

    /// Total number of records to process
    pub total_records: i64,

    /// Number of records processed so far
    pub processed: i64,

    /// Last processed ID (for ID-based pagination)
    pub last_id: i64,

    /// List of broken URL IDs found so far
    pub broken_ids: Vec<i64>,

    /// Total broken URLs found
    pub broken_count: i64,

    /// Timestamp when processing started
    pub started_at: DateTime<Utc>,

    /// Timestamp of last update
    pub updated_at: DateTime<Utc>,

    /// Current batch number
    pub current_batch: i64,

    /// Whether dry-run mode is enabled
    pub dry_run: bool,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(table: &str, total_records: i64, dry_run: bool) -> Self {
        let now = Utc::now();
        Self {
            table: table.to_string(),
            total_records,
            processed: 0,
            last_id: 0,
            broken_ids: Vec::new(),
            broken_count: 0,
            started_at: now,
            updated_at: now,
            current_batch: 0,
            dry_run,
        }
    }

    /// Get checkpoint file path
    fn get_checkpoint_path() -> PathBuf {
        Path::new(CHECKPOINT_DIR).join(CHECKPOINT_FILE)
    }

    /// Check if a checkpoint file exists
    pub fn exists() -> bool {
        Self::get_checkpoint_path().exists()
    }

    /// Save checkpoint to file
    pub async fn save(&self) -> Result<()> {
        let dir = Path::new(CHECKPOINT_DIR);

        // Create checkpoint directory if it doesn't exist
        if !dir.exists() {
            fs::create_dir_all(dir).await?;
        }

        let path = Self::get_checkpoint_path();
        let json = serde_json::to_string_pretty(self)?;

        fs::write(&path, &json).await?;

        debug!(
            "Checkpoint saved: processed={}, last_id={}, broken={}",
            self.processed, self.last_id, self.broken_count
        );

        Ok(())
    }

    /// Load checkpoint from file
    pub async fn load() -> Result<Option<Self>> {
        let path = Self::get_checkpoint_path();

        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&path).await?;
        let checkpoint: Self = serde_json::from_str(&data)?;

        info!(
            "Loaded checkpoint: processed={}/{}, last_id={}, broken={}",
            checkpoint.processed,
            checkpoint.total_records,
            checkpoint.last_id,
            checkpoint.broken_count
        );

        Ok(Some(checkpoint))
    }

    /// Update checkpoint with new progress
    pub fn update(&mut self, processed: i64, last_id: i64, new_broken_ids: Vec<i64>) {
        self.processed = processed;
        self.last_id = last_id;
        self.broken_count += new_broken_ids.len() as i64;
        self.broken_ids.extend(new_broken_ids);
        self.updated_at = Utc::now();
        self.current_batch += 1;
    }

    /// Delete checkpoint file
    pub async fn delete() -> Result<()> {
        let path = Self::get_checkpoint_path();

        if path.exists() {
            fs::remove_file(&path).await?;
            info!("Checkpoint deleted");
        }

        // Also try to remove the directory if empty
        let dir = Path::new(CHECKPOINT_DIR);
        if dir.exists() {
            if let Ok(mut entries) = fs::read_dir(dir).await {
                if entries.next_entry().await?.is_none() {
                    let _ = fs::remove_dir(dir).await;
                }
            }
        }

        Ok(())
    }

    /// Validate checkpoint is compatible with current run
    pub fn validate(&self, table: &str, dry_run: bool) -> bool {
        if self.table != table {
            warn!(
                "Checkpoint table mismatch: expected {}, found {}",
                table, self.table
            );
            return false;
        }

        if self.dry_run != dry_run {
            warn!(
                "Checkpoint dry_run mismatch: expected {}, found {}",
                dry_run, self.dry_run
            );
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_new() {
        let cp = Checkpoint::new("test_table", 1000, false);
        assert_eq!(cp.table, "test_table");
        assert_eq!(cp.total_records, 1000);
        assert_eq!(cp.processed, 0);
        assert_eq!(cp.broken_count, 0);
    }

    #[test]
    fn test_checkpoint_update() {
        let mut cp = Checkpoint::new("test_table", 1000, false);
        cp.update(100, 100, vec![1, 2, 3]);

        assert_eq!(cp.processed, 100);
        assert_eq!(cp.last_id, 100);
        assert_eq!(cp.broken_count, 3);
        assert_eq!(cp.broken_ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_validate() {
        let cp = Checkpoint::new("test_table", 1000, false);

        assert!(cp.validate("test_table", false));
        assert!(!cp.validate("other_table", false));
        assert!(!cp.validate("test_table", true));
    }
}
