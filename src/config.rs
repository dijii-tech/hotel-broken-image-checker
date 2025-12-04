use anyhow::{anyhow, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "broken-image-checker")]
#[command(
    about = "High-performance CLI tool to detect and clean broken image URLs from PostgreSQL databases"
)]
#[command(version)]
pub struct Args {
    /// Django project path (reads .env for DB credentials)
    #[arg(long, conflicts_with = "db_url")]
    pub project_path: Option<PathBuf>,

    /// Direct database URL (alternative to project_path)
    /// Example: postgres://user:pass@host:5432/dbname
    #[arg(long, env = "DATABASE_URL")]
    pub db_url: Option<String>,

    /// Database table name to check
    #[arg(long, default_value = "hotel_hotelproviderimage")]
    pub table: String,

    /// Column name containing URLs
    #[arg(long, default_value = "url")]
    pub url_column: String,

    /// Primary key column name
    #[arg(long, default_value = "id")]
    pub id_column: String,

    /// Number of concurrent HTTP requests
    #[arg(long, default_value_t = 500)]
    pub concurrency: usize,

    /// HTTP request timeout in seconds
    #[arg(long, default_value_t = 10)]
    pub timeout: u64,

    /// Number of records to fetch per database batch
    #[arg(long, default_value_t = 10000)]
    pub batch_size: i64,

    /// Dry run mode - only report, don't delete
    #[arg(long)]
    pub dry_run: bool,

    /// Delete broken URLs from database
    #[arg(long)]
    pub delete: bool,

    /// Output file for broken URLs report (CSV format)
    #[arg(long)]
    pub output: Option<String>,

    /// Resume from last checkpoint
    #[arg(long)]
    pub resume: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

impl Args {
    /// Get database URL from either direct input or .env file
    pub fn get_db_url(&self) -> Result<String> {
        if let Some(url) = &self.db_url {
            return Ok(url.clone());
        }

        if let Some(path) = &self.project_path {
            return Self::parse_env_file(path);
        }

        Err(anyhow!(
            "Either --project-path or --db-url must be provided"
        ))
    }

    /// Parse Django .env file and construct PostgreSQL connection URL
    fn parse_env_file(project_path: &Path) -> Result<String> {
        let env_path = project_path.join(".env");
        let content = std::fs::read_to_string(&env_path)
            .map_err(|e| anyhow!("Failed to read .env at {:?}: {}", env_path, e))?;

        let mut host: Option<String> = None;
        let mut user: Option<String> = None;
        let mut password: Option<String> = None;
        let mut name: Option<String> = None;
        let mut port = "5432".to_string();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "DB_HOST" => host = Some(value.to_string()),
                    "DB_USER" => user = Some(value.to_string()),
                    "DB_PASSWORD" => password = Some(value.to_string()),
                    "DB_NAME" => name = Some(value.to_string()),
                    "DB_PORT" => port = value.to_string(),
                    _ => {}
                }
            }
        }

        let host = host.ok_or_else(|| anyhow!("DB_HOST not found in .env"))?;
        let user = user.ok_or_else(|| anyhow!("DB_USER not found in .env"))?;
        let password = password.ok_or_else(|| anyhow!("DB_PASSWORD not found in .env"))?;
        let name = name.ok_or_else(|| anyhow!("DB_NAME not found in .env"))?;

        // URL encode special characters in password (e.g., @, $, #, etc.)
        let encoded_password = urlencoding::encode(&password);

        Ok(format!(
            "postgres://{}:{}@{}:{}/{}",
            user, encoded_password, host, port, name
        ))
    }

    /// Validate arguments
    pub fn validate(&self) -> Result<()> {
        if self.project_path.is_none() && self.db_url.is_none() {
            return Err(anyhow!(
                "Either --project-path or --db-url must be provided"
            ));
        }

        if self.concurrency == 0 {
            return Err(anyhow!("Concurrency must be greater than 0"));
        }

        if self.batch_size <= 0 {
            return Err(anyhow!("Batch size must be greater than 0"));
        }

        if self.timeout == 0 {
            return Err(anyhow!("Timeout must be greater than 0"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_env_file() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join(".env");

        let mut file = std::fs::File::create(&env_path).unwrap();
        writeln!(file, "DB_HOST=localhost").unwrap();
        writeln!(file, "DB_USER=testuser").unwrap();
        writeln!(file, "DB_PASSWORD=test@pass#123").unwrap();
        writeln!(file, "DB_NAME=testdb").unwrap();
        writeln!(file, "DB_PORT=5433").unwrap();

        let result = Args::parse_env_file(temp_dir.path()).unwrap();

        assert!(result.contains("localhost"));
        assert!(result.contains("testuser"));
        assert!(result.contains("5433"));
        assert!(result.contains("testdb"));
        // Password should be URL encoded
        assert!(result.contains("test%40pass%23123"));
    }

    #[test]
    fn test_parse_env_file_with_comments() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join(".env");

        let mut file = std::fs::File::create(&env_path).unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "DB_HOST=localhost").unwrap();
        writeln!(file, "#DB_HOST=ignored").unwrap();
        writeln!(file, "DB_USER=testuser").unwrap();
        writeln!(file, "DB_PASSWORD=testpass").unwrap();
        writeln!(file, "DB_NAME=testdb").unwrap();

        let result = Args::parse_env_file(temp_dir.path()).unwrap();

        assert!(result.contains("localhost"));
        assert!(!result.contains("ignored"));
    }
}
