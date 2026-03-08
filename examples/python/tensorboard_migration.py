"""
TensorBoard Migration Example
===============================

This example demonstrates how to migrate from TensorBoard's SummaryWriter
to expman's drop-in replacement. The API is identical — just change the
import line.

Usage::

    python examples/python/tensorboard_migration.py

After running, start the dashboard to visualize::

    exp serve ./runs
"""

import math
import random

from expman import SummaryWriter


def simulate_training():
    """Simulate a training loop, logging metrics with the TensorBoard API."""

    # ── 1. Create a SummaryWriter (same API as tensorboard) ─────────────
    writer = SummaryWriter(log_dir="runs/tb_migration_demo")

    # ── 2. Log hyperparameters ──────────────────────────────────────────
    writer.add_hparams(
        hparam_dict={"lr": 0.001, "batch_size": 64, "optimizer": "adam"},
        metric_dict={"hparam/final_loss": 0.0},  # placeholder, updated later
    )

    # ── 3. Training loop ────────────────────────────────────────────────
    epochs = 50
    for epoch in range(epochs):
        # Simulated metrics
        train_loss = 2.0 * math.exp(-0.05 * epoch) + random.gauss(0, 0.1)
        train_acc = 1.0 - math.exp(-0.08 * epoch) + random.gauss(0, 0.02)
        val_loss = 2.0 * math.exp(-0.04 * epoch) + random.gauss(0, 0.15)
        val_acc = 1.0 - math.exp(-0.06 * epoch) + random.gauss(0, 0.03)

        # Log individual scalars (TensorBoard style)
        writer.add_scalar("train/loss", train_loss, epoch)
        writer.add_scalar("train/accuracy", train_acc, epoch)

        # Log grouped scalars (TensorBoard style)
        writer.add_scalars(
            "validation",
            {"loss": val_loss, "accuracy": val_acc},
            epoch,
        )

        # Log text notes at key epochs
        if epoch % 10 == 0:
            writer.add_text("notes", f"Epoch {epoch}: lr warmup complete", epoch)

        if epoch % 25 == 0:
            print(
                f"Epoch {epoch:3d}/{epochs} — "
                f"train_loss={train_loss:.4f}, val_acc={val_acc:.4f}"
            )

    # ── 4. Stub methods (no-ops, won't crash) ───────────────────────────
    # These are silently ignored so existing TB code keeps working:
    writer.add_histogram("weights", [1.0, 2.0, 3.0], 0)
    writer.add_image("sample", [[[0]]], 0)

    # ── 5. Close the writer ─────────────────────────────────────────────
    writer.close()
    print("\nDone! View results with: exp serve ./runs")


def context_manager_example():
    """Show SummaryWriter used as a context manager."""

    with SummaryWriter(log_dir="experiments/ctx_manager_demo") as writer:
        for step in range(20):
            writer.add_scalar("metric", step * 0.1, step)
    # Writer is automatically closed here

    print("Context manager example complete.")


if __name__ == "__main__":
    simulate_training()
    context_manager_example()
