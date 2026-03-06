# ExpMan CLI

The `cli` module implements the `exp` command-line tool, providing a user-friendly interface for managing experiments from the terminal.

## Commands

- **`serve`**: Start the web dashboard server.
- **`list`**: List experiments and their runs.
- **`inspect`**: View detailed configuration and metrics for a specific run.
- **`clean`**: Prune old runs to save storage space.
- **`export`**: Export metric data to CSV or JSON format.

## Implementation

The CLI is built using the `clap` crate for robust argument parsing and `comfy-table` for beautiful terminal output. It shares the same core logic as the API server, ensuring consistency across all interfaces.

## Usage

```bash
exp list ./experiments
exp serve ./experiments --port 8080
```
