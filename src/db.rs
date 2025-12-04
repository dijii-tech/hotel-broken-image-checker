use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use tracing::{debug, info};

/// Database connection and operations handler
pub struct Database {
    pool: PgPool,
    table: String,
    id_column: String,
    url_column: String,
}

impl Database {
    /// Create a new database connection pool
    pub async fn new(
        db_url: &str,
        table: String,
        id_column: String,
        url_column: String,
    ) -> Result<Self> {
        info!("Connecting to database...");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(db_url)
            .await?;

        info!("Database connection established");

        Ok(Self {
            pool,
            table,
            id_column,
            url_column,
        })
    }

    /// Get total count of records with non-null URLs
    pub async fn get_total_count(&self) -> Result<i64> {
        let query = format!(
            "SELECT COUNT(*) as count FROM {} WHERE {} IS NOT NULL AND {} != ''",
            self.table, self.url_column, self.url_column
        );

        let row = sqlx::query(&query).fetch_one(&self.pool).await?;

        let count: i64 = row.get("count");
        debug!("Total records with URLs: {}", count);

        Ok(count)
    }

    /// Fetch a batch of URLs starting from a given ID
    /// Uses ID-based pagination which is more efficient than OFFSET for large tables
    pub async fn fetch_batch(&self, start_id: i64, limit: i64) -> Result<Vec<(i64, String)>> {
        let query = format!(
            "SELECT {}, {} FROM {} WHERE {} > $1 AND {} IS NOT NULL AND {} != '' ORDER BY {} LIMIT $2",
            self.id_column,
            self.url_column,
            self.table,
            self.id_column,
            self.url_column,
            self.url_column,
            self.id_column
        );

        let rows = sqlx::query(&query)
            .bind(start_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let results: Vec<(i64, String)> = rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get(0);
                let url: String = row.get(1);
                (id, url)
            })
            .collect();

        debug!(
            "Fetched {} records starting from ID {}",
            results.len(),
            start_id
        );

        Ok(results)
    }

    /// Delete records by their IDs in batches
    pub async fn delete_by_ids(&self, ids: &[i64]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut total_deleted: u64 = 0;

        // Process in chunks of 1000 to avoid query size limits
        for chunk in ids.chunks(1000) {
            let query = format!(
                "DELETE FROM {} WHERE {} = ANY($1)",
                self.table, self.id_column
            );

            let result = sqlx::query(&query)
                .bind(chunk)
                .execute(&self.pool)
                .await?;

            total_deleted += result.rows_affected();
        }

        info!("Deleted {} records from database", total_deleted);

        Ok(total_deleted)
    }

    /// Close the database connection pool
    pub async fn close(self) {
        self.pool.close().await;
        info!("Database connection closed");
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would require a running PostgreSQL instance
    // Unit tests can mock the database interactions
}
