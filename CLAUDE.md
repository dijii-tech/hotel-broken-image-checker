# Hotel Broken Image Checker - Project Guide

## Project Overview

High-performance Rust CLI tool for detecting and cleaning broken image URLs from PostgreSQL databases. Designed for large-scale operations (17M+ records) with async processing.

## Architecture

```
src/
├── main.rs       # CLI entry point, argument parsing (clap)
├── config.rs     # Configuration handling, .env parsing
├── db.rs         # PostgreSQL operations (sqlx), batch queries
├── checker.rs    # HTTP URL validation (reqwest), async workers
└── checkpoint.rs # Progress persistence, resume functionality
```

## Key Technologies

- **Async Runtime**: Tokio (full features)
- **HTTP Client**: reqwest with rustls-tls
- **Database**: sqlx with PostgreSQL
- **CLI**: clap with derive macros
- **Progress**: indicatif for progress bars
- **Serialization**: serde + serde_json

## Development Commands

```bash
# Build debug
cargo build

# Build release (optimized)
cargo build --release

# Run tests
cargo test

# Check without building
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

## Release Process

1. Update version in `Cargo.toml`
2. Commit changes
3. Create and push tag:
   ```bash
   git tag -a v1.x.x -m "Release v1.x.x"
   git push origin v1.x.x
   ```
4. GitHub Actions automatically builds binaries for:
   - Linux x86_64, ARM64, musl
   - macOS Intel, Apple Silicon

## Code Patterns

### Async HTTP Checking
- Uses `futures::stream::buffer_unordered` for concurrent requests
- Configurable concurrency (default: 500)
- HEAD request first, fallback to GET
- 3-phase retry mechanism for temporary errors

### Retry System
- Phase 1: Normal concurrent check
- Phase 2: Immediate retry for retryable errors
- Phase 3: Delayed retry (configurable delay)
- Non-retryable: 404, 400, 401, 403, 410, 451
- Retryable: All other errors including timeouts

### Database Operations
- Batch fetching with configurable batch size
- Prepared statements for performance
- Automatic backup before deletion (`{table}_deleted_backup`)
- Transaction-based deletions

### Checkpoint System
- JSON-based progress file in `.checkpoint/`
- Tracks: processed count, broken IDs, last ID
- Enables resume after interruption

## Common Tasks

### Adding New CLI Option
1. Add field to `Config` struct in `config.rs`
2. Add clap attribute for CLI parsing
3. Use in relevant module

### Modifying HTTP Logic
- Edit `checker.rs`
- `check_url()` handles single URL
- `check_urls_batch()` handles concurrent checking

### Database Schema Changes
- Edit `db.rs`
- Update queries in `fetch_urls_batch()` and `delete_broken_urls()`

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

## Performance Tuning

| Parameter | Flag | Default | Notes |
|-----------|------|---------|-------|
| Concurrency | `--concurrency` | 500 | HTTP workers |
| Batch Size | `--batch-size` | 10000 | DB fetch size |
| Timeout | `--timeout` | 10s | Per-request timeout |
| Retry Attempts | `--retry-attempts` | 2 | Number of retries |
| Retry Delay | `--retry-delay` | 10s | Phase 3 wait time |

## Troubleshooting

- **Memory issues**: Reduce `--batch-size`
- **Rate limiting (429)**: Reduce `--concurrency`
- **Timeouts**: Increase `--timeout`
- **Resume fails**: Check `.checkpoint/progress.json`
- **False positives (503)**: Increase `--retry-attempts` and `--retry-delay`
- **Restore deleted**: Query `{table}_deleted_backup` table
