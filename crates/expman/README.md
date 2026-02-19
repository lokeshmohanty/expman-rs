# ExpMan CLI

## Overview

The `expman-cli` crate provides the command-line interface for ExpMan. It allows users to manage experiments, start the server, and perform other administrative tasks directly from the terminal.

## Key Features

- **Experiment Management**: Create, list, and delete experiments.
- **Server Control**: Start and stop the ExpMan server.
- **Configuration**: Manage global configuration settings.

## Usage

Install the CLI using `cargo install`:

```bash
cargo install expman-cli
```

### Commands

- `expman serve`: Start the ExpMan server.
- `expman list`: List all experiments.
- `expman help`: Show available commands.

## API Documentation

For detailed API documentation, run `cargo doc --open`.
