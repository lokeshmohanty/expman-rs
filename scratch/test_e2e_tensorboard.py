import sys
import os
import time

# Ensure we import the local dev package
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../wrappers/python")))

from expman import Experiment
import tensorboardX

def main():
    print("Initializing E2E dummy experiment...")
    # Use 'test_experiments' directory inside the workspace
    with Experiment("e2e_tb_experiment", base_dir="./test_experiments") as exp:
        print(f"Created run: {exp.run_name}")
        
        # 1. Log parameters
        exp.log_params({
            "learning_rate": 0.005,
            "batch_size": 32,
            "architecture": "resnet18",
            "e2e_test": True
        })
        
        # 2. Log vector metrics
        for step in range(15):
            exp.log_vector({
                "loss": 2.5 / (step + 1),
                "accuracy": 0.3 + 0.6 * (step / 15)
            }, step=step)
            time.sleep(0.05)
            
        # 3. Log scalar metrics
        exp.log_scalar("final_loss", 0.15)
        exp.log_scalar("final_accuracy", 0.91)
        
        # 4. Save a small artifact
        os.makedirs("test_artifacts", exist_ok=True)
        with open("test_artifacts/weights.txt", "w") as f:
            f.write("dummy weights for e2e validation")
        exp.save_artifact("test_artifacts/weights.txt")
        
        # 5. Log actual TensorBoard data using tensorboardX
        tb_dir = exp.tensorboard_dir
        print(f"Logging TensorBoard to: {tb_dir}")
        
        writer = tensorboardX.SummaryWriter(log_dir=tb_dir)
        for step in range(30):
            writer.add_scalar("tb_metric/loss", 3.0 * (0.9 ** step), step)
            writer.add_scalar("tb_metric/precision", 0.4 + 0.5 * (step / 30), step)
        writer.close()
        
    print("E2E dummy experiment data generation complete.")

if __name__ == "__main__":
    main()
