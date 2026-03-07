# ExpMan CLI

The `cli` module implements the `exp` command-line tool, providing a user-friendly interface for managing experiments from the terminal.

## Commands

- **`serve`**: Start the web dashboard server.
- **`list`**: List experiments and their runs.
- **`inspect`**: View detailed configuration and metrics for a specific run.
- **`clean`**: Prune old runs to save storage space.
- **`export`**: Export metric data to CSV, JSON, or TensorBoard format.
- **`import`**: Import metrics from TensorBoard event logs into expman.

## TensorBoard Interoperability

### Importing TensorBoard Logs

Convert existing TensorBoard event files into expman experiments:

```bash
# Import from a directory containing tfevents files
exp import /path/to/tb_logs --dir ./experiments

# The directory name becomes the experiment name
exp import ./runs/resnet_cifar10 --dir ./experiments
```

The importer reads scalar summaries from `tfevents` files and creates a new
expman run with the metrics stored in `vectors.parquet`.

### Exporting to TensorBoard

Export expman metrics as TensorBoard-compatible event files:

```bash
# Export a run to TensorBoard format
exp export ./experiments/resnet/20240101_120000 --format tensorboard --output ./tb_logs

# Then visualize with TensorBoard
tensorboard --logdir ./tb_logs
```

## Implementation

The CLI is built using the `clap` crate for robust argument parsing and `comfy-table` for beautiful terminal output. It shares the same core logic as the API server, ensuring consistency across all interfaces.

TensorBoard import uses the `tboard` crate to read event files, and export uses `tensorboard-rs` to write them.

## Usage

```bash
exp list ./experiments
exp serve ./experiments --port 8080
exp export ./experiments/my_exp/20240101_120000 --format csv
exp import /path/to/tb_logs --dir ./experiments
```
