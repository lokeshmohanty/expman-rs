# Expman CLI

Friendly command-line interface for managing and inspecting your `expman-rs` experiments.

## Overview

The `expman` CLI tool provides various commands to serve the web dashboard, inspect experiment runs, clean up old runs, and export metrics.

## Commands

### `serve`
Start the web dashboard server to view experiments and real-time metrics.

```bash
expman serve [OPTIONS]
```
**Options:**
- `[DIR]`: Path to experiments directory (default: `./experiments`)
- `--host`: Host to bind to (default: `127.0.0.1`)
- `--port, -p`: Port to bind to (default: `8000`)
- `--no-live`: Disable live SSE streaming

### `list`
List all experiments, or runs for a specific experiment.

```bash
expman list [OPTIONS]
```
**Options:**
- `[DIR]`: Path to experiments directory (default: `./experiments`)
- `--experiment, -e <EXP>`: Show runs for a specific experiment

### `inspect`
Inspect a specific run, showing its configuration, metadata, and the last recorded metrics.

```bash
expman inspect <RUN_DIR>
```

### `clean`
Remove old runs, keeping only the N most recent to save disk space.

```bash
expman clean <EXPERIMENT> [OPTIONS]
```
**Options:**
- `--dir`: Path to experiments directory (default: `./experiments`)
- `--keep, -k <N>`: Number of most recent runs to keep (default: `5`)
- `--force`: Actually delete the runs (default is dry-run)

### `export`
Export metrics from a particular run to CSV or JSON formats.

```bash
expman export <RUN_DIR> [OPTIONS]
```
**Options:**
- `--format, -f <csv|json>`: Output format (default: `csv`)
- `--output, -o <FILE>`: Output file (default prints to stdout)
