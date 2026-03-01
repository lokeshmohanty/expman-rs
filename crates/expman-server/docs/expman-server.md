# Expman Server

Axum web server with REST API and SSE live streaming for `expman-rs`.

## Overview

The `expman-server` module provides a lightweight, high-performance web backend built with Axum. It serves both the REST API for retrieving experiment metadata/metrics and the Server-Sent Events (SSE) endpoints for real-time live streaming of experiment updates directly to the web dashboard.

## Key Features

- **Live Data Streaming (SSE):** Clients receive real-time updates for logs and metrics via Server-Sent Events, removing the need for continuous polling.
- **Embedded Frontend:** The compiled web dashboard UI is embedded directly into the binary, simplifying deployment.
- **RESTful API:** Provides clean JSON endpoints for querying experiments, runs, artifacts, and historical Parquet metric data.
- **Jupyter Integration:** Includes endpoints to automatically spawn pre-configured live Jupyter notebooks analyzing a specific run's data.

## Server Usage

You typically do not need to use `expman-server` directly in your code, as the `expman-cli` binary wraps it perfectly.

To run the server via the CLI wrapper:
```bash
# Starts the server on http://localhost:8000
expman serve ./experiments
```

## API Endpoints Overview

If you wish to interact programmatically with the dashboard's data, the server exposes the following routes under `/api`:

### Experiments & Runs
- `GET /api/experiments`: List all registered experiments.
- `GET /api/experiments/:experiment/runs`: List all runs for a given experiment. Supports query parameter filtering like `?metrics=loss,accuracy` to selectively extract specific scalar tails.
- `GET /api/experiments/:experiment/runs/:run`: Retrieve full details (metadata, full config) of a specific run.

### Data & Files
- `GET /api/experiments/:experiment/runs/:run/metrics`: Fetch historical metric data for a run (parsed from `metrics.parquet`).
- `GET /api/experiments/:experiment/runs/:run/artifacts`: List all files saved in a run's `artifacts/` folder.
- `GET /api/experiments/:experiment/runs/:run/artifacts/*path`: Download a specific artifact file.

### Live SSE Streams
- `GET /api/experiments/:experiment/runs/:run/live`: Connect to the Server-Sent Event stream for a specific run. Streams events like `LogMessage`, `MetricsUpdated`, and `StatusChanged` as they happen.

### Integrations
- `POST /api/jupyter/spawn`: Spawns a Jupyter notebook environment tied to a specific run for live analysis using polars.
