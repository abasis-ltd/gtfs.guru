# Usage

## Command Line Interface (CLI)

### Basic Usage

```bash
# Validate local file

gtfs-guru --input /path/to/gtfs.zip --output_base ./report

# Validate from URL
gtfs-guru --url https://example.com/gtfs.zip --output_base ./report
```

### CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--input <PATH>` | `-i` | Path to GTFS zip file or directory |
| `--url <URL>` | `-u` | URL to download GTFS feed |
| `--output_base <DIR>` | `-o` | Output directory for reports (required) |
| `--country_code <CODE>` | `-c` | ISO country code (e.g., US, RU, DE) |
| `--date <DATE>` | `-d` | Validation date (YYYY-MM-DD) |
| `--pretty` | `-p` | Format JSON output |
| `--export_notices_schema` | `-n` | Export notice schema to JSON |
| `--storage_directory <DIR>` | `-s` | Save downloaded feed to directory |
| `--validation_report_name <NAME>` | `-v` | Custom name for JSON report |
| `--html_report_name <NAME>` | `-r` | Custom name for HTML report |
| `--threads <N>` | | Number of threads (default: 1) |

## Web API

### Starting the Server

```bash
cargo run --release -p gtfs-guru-web
# Server starts at http://localhost:3000
```

### API Endpoints

- `GET /healthz` - Health check
- `GET /version` - Version info
- `POST /create-job` - Create validation job
- `PUT /upload/{job_id}` - Upload GTFS file
- `GET /jobs/{job_id}/status` - Check status
- `GET /jobs/{job_id}/report.json` - JSON report
- `GET /jobs/{job_id}/report.html` - HTML report
