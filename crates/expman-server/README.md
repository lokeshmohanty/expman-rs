# ExpMan Server

## Overview

The `expman-server` crate implements the backend server for the ExpMan web dashboard. It provides a RESTful API for interacting with experiment data and serves the frontend application.

## Key Features

- **REST API**: Exposes endpoints for creating, retrieving, and updating experiments.
- **WebSocket Support**: Enables real-time updates for experiment monitoring.
- **Static File Serving**: Serves the compiled frontend assets.
- **Database Integration**: Connects to the experiment database (SQLite/Parquet).

## Usage

To start the server, use the `expman-cli` tool:

```bash
expman serve
```

## API Documentation

For detailed API documentation, run `cargo doc --open`.
