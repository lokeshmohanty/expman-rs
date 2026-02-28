# Jupyter Notebook Integration

The `expman-rs` dashboard includes powerful, seamless integration with **Jupyter Notebooks**, allowing you to instantly analyze run data without leaving your browser.

## Overview

When exploring the details of a specific Run in the dashboard, you can open the **Interactive** tab. This tab embeds a fully functional, live Jupyter Notebook interface directly in the UI.

The dashboard intelligently spins up a background Jupyter instance strictly tied to that run's execution folder, securely exposing an ephemeral port just for your dashboard session. Everything is automatically destroyed when you click "Stop Notebook".

## Requirements

To use this feature, **Jupyter Notebook must be installed in the environment where you start the ExpMan Dashboard**.

Because `expman-rs` uses your local environment to spawn the background notebook, simply having Python installed isn't enough; the `jupyter` command must be on your `PATH`.

```bash
# Example: Install Jupyter with uv directly into the dashboard's environment
uv tool install notebook

# Alternatively, using pip:
pip install notebook
```

If Jupyter is not detected, the "Launch Live Analysis" button will be disabled, and the dashboard will provide a helpful warning with instructions to install it.

## The Interactive Experience

1. **Auto-Generated Boilerplate**: 
   When you select a run, the system automatically writes an `interactive.ipynb` file directly into that run's folder on disk if it doesn't already exist.
   
2. **Polars for Analytics**:
   By default, the provided notebook template utilizes [Polars](https://pola.rs) (`pl.read_parquet`) to ensure ultra-fast load times for large metrics files, as opposed to pandas.

3. **In-Notebook Requirements**:
   The notebook's first cell explicitly helps you safely install the required analytical tools (`polars`, `matplotlib`, `pyarrow`, `fastparquet`) directly into your current python environment from within the notebook without breaking the terminal running the dashboard. It uses the safe `!pip install ... --python {sys.executable}` approach.

## Troubleshooting

- **Address Already in Use**: If you previously crashed the dashboard abruptly while a notebook was running, the zombie `jupyter notebook` process might still occupy the allocated port.
- **Port Ranges**: `expman-rs` searches for available ports starting from `8000` to `9000` to bind new Notebooks. Ensure this range isn't strictly blocked by local firewalls.
