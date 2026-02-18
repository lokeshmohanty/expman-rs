"""
Basic training example using expman-rs.
This script demonstrates logging parameters, metrics, and saving artifacts.
"""

import time
import random
from expman import Experiment

def main():
    # 1. Initialize the experiment
    # All logging is non-blocking (channel send ~100ns)
    with Experiment("mnist_classifier", base_dir="./experiments") as exp:
        print(f"Starting run: {exp.run_name}")
        
        # 2. Log hyperparameters
        params = {
            "learning_rate": 0.001,
            "batch_size": 64,
            "optimizer": "adam",
            "model": "cnn_v2"
        }
        exp.log_params(params)
        print("Logged hyper-parameters.")

        # 3. Simulate training loop
        epochs = 20
        for epoch in range(epochs):
            # Simulate work
            time.sleep(0.5)
            
            # Simulate metrics
            train_loss = 1.0 / (epoch + 1) + random.random() * 0.1
            train_acc = 0.5 + (0.4 * (epoch / epochs)) + random.random() * 0.05
            
            # Log metrics (non-blocking)
            exp.log_metrics({
                "train/loss": train_loss,
                "train/acc": train_acc,
            }, step=epoch)
            
            # Log periodic validation metrics
            if epoch % 5 == 0:
                val_acc = train_acc - 0.05 + random.random() * 0.02
                exp.log_metrics({"val/acc": val_acc}, step=epoch)
                exp.info(f"Epoch {epoch}: val_acc={val_acc:.4f}")

        # 4. Save an artifact
        # We'll just create a dummy file to save
        with open("model.pt", "w") as f:
            f.write("dummy model weights")
            
        exp.save_artifact("model.pt")
        print("Saved artifact.")
        
    print("Experiment finished. Start the dashboard with: expman serve ./experiments")

if __name__ == "__main__":
    main()
