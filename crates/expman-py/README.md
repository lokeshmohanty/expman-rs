# ExpMan Python Bindings

## Overview

The `expman-py` crate provides Python bindings for the ExpMan core functionality. It allows users to integrate ExpMan into their Python-based machine learning workflows seamlessly.

## Key Features

- **High Performance**: Leverages Rust for critical performance paths.
- **Easy Integration**: Simple Python API that feels native.
- **Type Safety**: Fully typed for better developer experience.

## Installation

Install via pip:

```bash
pip install expman
```

## Usage

```python
import expman

# Create an experiment
exp = expman.Experiment(name="my-experiment")

# Log metrics
exp.log({"accuracy": 0.95, "loss": 0.05})
```

## API Documentation

See the main documentation site for full API details.
