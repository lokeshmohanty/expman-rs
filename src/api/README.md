# ExpMan Backend API

The `api` module provides the RESTful interface and real-time streaming services for the ExpMan dashboard.

## Key Features

- **Axum Framework**: A modern, high-performance web framework used to build robust and scalable API endpoints.
- **SSE Streaming**: Server-Sent Events are used to stream live metric updates directly from the logging engine to the dashboard.
- **Jupyter Integration**: API handlers for spawning and managing ephemeral Jupyter Notebook instances for live analysis.
- **Embedded Assets**: The frontend Leptos application is embedded directly into the binary using `rust-embed`.

## Main Endpoints

- `/api/experiments`: List all experiments and their metadata.
- `/api/stats`: Global statistics for all experiments.
- `/api/jupyter`: Management of Jupyter Notebook sessions.
- `/api/events`: Real-time SSE event stream.

## Documentation Implementation

The module also implements custom documentation using `axum` and `utoipa` (if enabled in the future) to describe the API structure.
