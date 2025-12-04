mod checker;
mod checkpoint;
mod config;
mod db;

use anyhow::Result;
use checker::{CheckResult, UrlChecker};
use checkpoint::Checkpoint;
use clap::Parser;
use config::Args;
use csv::Writer;
use db::Database;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Setup logging
    let log_level = if args.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .with_thread_ids(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Validate arguments
    args.validate()?;

    // Get database URL
    let db_url = args.get_db_url()?;
    info!("Database URL configured successfully");

    // Connect to database
    let database = Database::new(
        &db_url,
        args.table.clone(),
        args.id_column.clone(),
        args.url_column.clone(),
    )
    .await?;

    // Get total count
    let total_count = database.get_total_count().await?;
    info!("Total records with URLs: {}", total_count);

    if total_count == 0 {
        info!("No records to process");
        return Ok(());
    }

    // Handle resume logic
    let mut checkpoint = if args.resume && Checkpoint::exists() {
        match Checkpoint::load().await? {
            Some(cp) if cp.validate(&args.table, args.dry_run) => {
                info!(
                    "Resuming from checkpoint: {}/{} processed, starting from ID {}",
                    cp.processed, cp.total_records, cp.last_id
                );
                cp
            }
            Some(_) => {
                warn!("Checkpoint validation failed, starting fresh");
                Checkpoint::new(&args.table, total_count, args.dry_run)
            }
            None => {
                warn!("Could not load checkpoint, starting fresh");
                Checkpoint::new(&args.table, total_count, args.dry_run)
            }
        }
    } else {
        if args.resume {
            info!("No checkpoint found, starting fresh");
        }
        Checkpoint::new(&args.table, total_count, args.dry_run)
    };

    // Create URL checker with retry configuration
    let checker = UrlChecker::new(
        args.concurrency,
        args.timeout,
        args.retry_attempts,
        args.retry_delay,
    )?;
    info!(
        "URL checker initialized with {} concurrent connections, {} retry attempts ({}s delay)",
        args.concurrency, args.retry_attempts, args.retry_delay
    );

    // Setup progress bar
    let pb = ProgressBar::new(total_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | Broken: {msg} | ETA: {eta}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_position(checkpoint.processed as u64);
    pb.set_message(format!("{}", checkpoint.broken_count));

    // CSV writer for output
    let mut csv_writer = if let Some(output_path) = &args.output {
        let file = File::create(output_path)?;
        let mut writer = Writer::from_writer(file);
        writer.write_record(["id", "url", "status_code", "error"])?;
        Some(writer)
    } else {
        None
    };

    // Process batches
    let mut last_id = checkpoint.last_id;
    let mut all_broken_ids: Vec<i64> = checkpoint.broken_ids.clone();

    info!("Starting URL check...");
    if args.dry_run {
        info!("DRY RUN MODE - No deletions will be performed");
    }

    loop {
        // Fetch batch
        let batch = database.fetch_batch(last_id, args.batch_size).await?;

        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len() as i64;

        // Update last_id for next iteration
        if let Some((id, _)) = batch.last() {
            last_id = *id;
        }

        // Check URLs in batch
        let results = checker.check_batch(batch).await;

        // Process results
        let broken_results: Vec<&CheckResult> = results.iter().filter(|r| !r.is_valid).collect();
        let broken_ids: Vec<i64> = broken_results.iter().map(|r| r.id).collect();

        // Write broken URLs to CSV if output is specified
        if let Some(ref mut writer) = csv_writer {
            for result in &broken_results {
                writer.write_record([
                    result.id.to_string(),
                    result.url.clone(),
                    result.status_code.map(|s| s.to_string()).unwrap_or_default(),
                    result.error.clone().unwrap_or_default(),
                ])?;
            }
            writer.flush()?;
        }

        // Collect broken IDs
        all_broken_ids.extend(&broken_ids);

        // Update checkpoint
        checkpoint.update(
            checkpoint.processed + batch_len,
            last_id,
            broken_ids,
        );

        // Save checkpoint periodically (every 10 batches)
        if checkpoint.current_batch % 10 == 0 {
            checkpoint.save().await?;
        }

        // Update progress bar
        pb.set_position(checkpoint.processed as u64);
        pb.set_message(format!("{}", checkpoint.broken_count));
    }

    pb.finish_with_message(format!("Done! {} broken URLs found", checkpoint.broken_count));

    // Save final checkpoint
    checkpoint.save().await?;

    // Summary
    info!("=== Summary ===");
    info!("Total records checked: {}", checkpoint.processed);
    info!("Broken URLs found: {}", checkpoint.broken_count);
    info!(
        "Broken rate: {:.2}%",
        (checkpoint.broken_count as f64 / checkpoint.processed as f64) * 100.0
    );

    if let Some(output_path) = &args.output {
        info!("Broken URLs exported to: {}", output_path);
    }

    // Delete broken URLs if requested
    if args.delete && !args.dry_run && !all_broken_ids.is_empty() {
        let backup = !args.no_backup;
        if backup {
            info!(
                "Backing up and deleting {} broken URL records...",
                all_broken_ids.len()
            );
        } else {
            warn!(
                "Deleting {} broken URL records WITHOUT backup...",
                all_broken_ids.len()
            );
        }

        let deleted = database.delete_by_ids(&all_broken_ids, backup).await?;
        info!("Successfully deleted {} records", deleted);

        if backup {
            info!(
                "Backup stored in table: {}_deleted_backup",
                args.table
            );
        }

        // Clean up checkpoint after successful deletion
        Checkpoint::delete().await?;
    } else if args.dry_run {
        info!("DRY RUN - Would delete {} records", all_broken_ids.len());
    } else if !args.delete {
        info!("Use --delete flag to remove broken URLs from database");
    }

    // Close database connection
    database.close().await;

    Ok(())
}
