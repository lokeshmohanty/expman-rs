"""
Singleton usage example for expman-rs.
This demonstrates tracking without a context manager (using expman.init()).
"""

import random
import time

import expman


def train():
    # 1. Initialize global experiment (replaces with Experiment(...) as exp)
    expman.init("singleton_example")

    print("Initiated global experiment.")

    # 2. Log parameters globally
    expman.log_params({"lr": 0.01, "batch_size": 32, "singleton": True})

    # 3. Log metrics anywhere without passing an 'exp' object
    for step in range(10):
        val = random.random()
        expman.log_metrics({"accuracy": val}, step=step)
        print(f"Step {step}: acc={val:.4f}")
        time.sleep(0.2)

    # 4. Save artifact globally
    with open("notes.txt", "w") as f:
        f.write("Using the singleton API!")

    expman.save_artifact("notes.txt")

    # 5. Optional manual close (atexit handles it otherwise)
    expman.close()
    print("Experiment finished and closed.")


if __name__ == "__main__":
    train()
