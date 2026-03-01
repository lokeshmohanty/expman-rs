# Expman Frontend

This module contains the Leptos-based web frontend for the Expman dashboard.

## Overview

The dashboard connects to the `expman-server` backend via REST APIs and Server-Sent Events (SSE) to display real-time metrics, runs lists, experiment details, and interactive analytics environments.

## Architecture

The frontend is built with:
- **[Leptos](https://docs.rs/leptos)**: A reactive, full-stack web framework in Rust.
- **[Tailwind CSS](https://tailwindcss.com)**: For styling.
- **Wasm**: Compiled into WebAssembly and embedded into the application binary.

### Key Components

- `App`: The main application router.
- `Dashboard`: The default view summarizing active and recent runs.
- `Experiments`: Listing available experiments.
- `ExperimentDetail`: In-depth view for a single experiment with tabs for metrics, artifacts, runs table, etc.
- `InteractiveView`: Allows spinning up Jupyter environments.
- `LineChart`: Wrapper around Plotly.js for real-time charting.
