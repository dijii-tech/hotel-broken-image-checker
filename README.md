# Broken Image Checker

High-performance Rust CLI tool to detect and clean broken image URLs from PostgreSQL databases.

## Features

- **High Performance**: 500+ concurrent HTTP requests using Tokio async runtime
- **Memory Efficient**: Processes 17M+ records with ~200-500MB memory
- **Resume Support**: Checkpoint system allows resuming interrupted operations
- **Django Integration**: Automatically reads database credentials from `.env` file
- **Flexible Output**: CSV export for analysis, direct database deletion
- **Progress Tracking**: Real-time progress bar with ETA

## Installation

### Option 1: Download Pre-built Binary (Recommended)

Download the latest release for your platform from [GitHub Releases](https://github.com/dijii-tech/hotel-broken-image-checker/releases):

| Platform | Architecture | Download |
|----------|--------------|----------|
| Linux | x86_64 | `hotel-broken-image-checker-linux-x86_64.tar.gz` |
| Linux | x86_64 (static) | `hotel-broken-image-checker-linux-x86_64-musl.tar.gz` |
| Linux | ARM64 | `hotel-broken-image-checker-linux-aarch64.tar.gz` |
| macOS | Intel | `hotel-broken-image-checker-macos-x86_64.tar.gz` |
| macOS | Apple Silicon | `hotel-broken-image-checker-macos-aarch64.tar.gz` |

```bash
# Download and extract (example for Linux x86_64)
wget https://github.com/dijii-tech/hotel-broken-image-checker/releases/latest/download/hotel-broken-image-checker-linux-x86_64.tar.gz
tar -xzf hotel-broken-image-checker-linux-x86_64.tar.gz

# Make executable and move to PATH
chmod +x hotel-broken-image-checker
sudo mv hotel-broken-image-checker /usr/local/bin/

# Verify installation
hotel-broken-image-checker --help
```

### Option 2: Build from Source

#### Prerequisites

- Rust 1.70+ ([Install Rust](https://rustup.rs/))
- PostgreSQL database access

#### Build

```bash
git clone https://github.com/dijii-tech/hotel-broken-image-checker.git
cd hotel-broken-image-checker
cargo build --release
```

The binary will be at `./target/release/hotel-broken-image-checker`

## Usage

### Basic Usage (with Django .env)

```bash
# Dry run - check URLs and export to CSV
./target/release/broken-image-checker \
  --project-path "/path/to/django/project" \
  --table "hotel_hotelproviderimage" \
  --dry-run \
  --output broken_urls.csv

# Delete broken URLs
./target/release/broken-image-checker \
  --project-path "/path/to/django/project" \
  --table "hotel_hotelproviderimage" \
  --delete
```

### With Direct Database URL

```bash
./target/release/broken-image-checker \
  --db-url "postgres://user:pass@host:5432/dbname" \
  --table "hotel_hotelproviderimage" \
  --delete
```

### Resume Interrupted Operation

```bash
./target/release/broken-image-checker \
  --project-path "/path/to/django/project" \
  --table "hotel_hotelproviderimage" \
  --delete \
  --resume
```

## Command Line Options

| Option | Description | Default |
|--------|-------------|---------|
| `--project-path` | Django project path (reads .env) | - |
| `--db-url` | Direct PostgreSQL URL | - |
| `--table` | Database table name | `hotel_hotelproviderimage` |
| `--url-column` | URL column name | `url` |
| `--id-column` | Primary key column | `id` |
| `--concurrency` | Concurrent HTTP requests | `500` |
| `--timeout` | HTTP timeout (seconds) | `10` |
| `--batch-size` | Records per batch | `10000` |
| `--dry-run` | Report only, no deletion | `false` |
| `--delete` | Delete broken URLs | `false` |
| `--output` | CSV output file path | - |
| `--resume` | Resume from checkpoint | `false` |
| `-v, --verbose` | Verbose logging | `false` |

## .env File Format

The tool expects these variables in your Django `.env` file:

```env
DB_HOST=localhost
DB_USER=django_user
DB_PASSWORD=your_password
DB_NAME=database_name
DB_PORT=5432  # Optional, defaults to 5432
```

## Performance

| Records | Concurrency | Estimated Time |
|---------|-------------|----------------|
| 100K | 500 | ~3-5 minutes |
| 1M | 500 | ~30-45 minutes |
| 17M | 500 | ~1-2 hours |

Performance depends on:
- Network latency to image servers
- Server response times
- Rate limiting by image providers

## Checkpoint System

Progress is automatically saved to `.checkpoint/progress.json`:

```json
{
  "table": "hotel_hotelproviderimage",
  "total_records": 17000000,
  "processed": 5430000,
  "last_id": 5430000,
  "broken_ids": [...],
  "broken_count": 12500,
  "started_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T11:45:00Z"
}
```

Use `--resume` to continue from the last checkpoint after interruption.

## Output CSV Format

When using `--output`, broken URLs are exported:

```csv
id,url,status_code,error
12345,https://example.com/image.jpg,404,
67890,https://broken.com/img.png,,Connection failed
```

## Notes

- Always run with `--dry-run` first to preview results
- Some servers don't support HEAD requests; the tool falls back to GET
- Rate limiting from providers may slow down checks (429 errors)
- The checkpoint is deleted after successful `--delete` operation

## License

MIT
