"""
Basic training example using expman-rs.
This script demonstrates logging parameters, metrics, and saving artifacts.
"""

import random
import time

from expman import Experiment


def main():
    # 1. Initialize the experiment
    # All logging is non-blocking (channel send ~100ns)
    with Experiment("test_example", base_dir="./experiments") as exp:
        print(f"Starting run: {exp.run_name}")

        # 2. Log hyperparameters
        params = {
            "learning_rate": 0.001,
            "batch_size": 64,
            "optimizer": "adam",
            "model": "cnn_v2",
        }
        exp.log_params(params)
        print("Logged hyper-parameters.")

        # 3. Simulate training loop
        epochs = 40
        for epoch in range(epochs):
            # Simulate work
            time.sleep(0.5)

            # Simulate metrics
            train_loss = 5.0 / (epoch + 1) + random.normalvariate() * 2.1
            train_acc = 0.5 + (0.4 * (epoch / epochs)) + random.random() * 0.05

            # Log metrics (non-blocking)
            exp.log_vector(
                {
                    "train/loss": train_loss,
                    "train/acc": train_acc,
                },
                step=epoch,
            )

            # Log periodic validation metrics
            if epoch % 5 == 0:
                val_acc = train_acc - 0.05 + random.random() * 0.02
                exp.log_vector({"val/acc": val_acc}, step=epoch)
                exp.info(f"Epoch {epoch}: val_acc={val_acc:.4f}")

        # 4. Log final summary metrics
        # These will be the "scalar metrics" visible in the Runs Table
        exp.log_scalar("loss", train_loss)
        exp.log_scalar("acc", train_acc)
        print(f"Logged final metrics: loss={train_loss:.4f}, acc={train_acc:.4f}")

        # 4. Save an artifact
        # We'll just create a dummy file to save
        with open("model.pt", "w") as f:
            f.write("dummy model weights")

        exp.save_artifact("model.pt")
        print("Saved model artifact.")

        # 5. Generate and save a matplotlib plot artifact
        import matplotlib.pyplot as plt
        import numpy as np

        t = np.linspace(0, 10, 100)
        plt.figure()
        plt.plot(t, np.sin(t), label="sin(t)")
        plt.plot(t, np.cos(t), label="cos(t)")
        plt.title("Dummy Plot")
        plt.legend()
        plt.savefig("plot.png")
        plt.close()
        exp.save_artifact("plot.png")
        print("Saved plot artifact (plot.png).")

        # 6. Generate and save a dummy image artifact (numpy array)
        width, height = 128, 128
        x, y = np.meshgrid(np.linspace(0, 1, width), np.linspace(0, 1, height))
        r = x
        g = y
        b = 0.5 + 0.5 * np.sin(10 * (x + y))
        img = np.stack([r, g, b], axis=-1)
        plt.imsave("gradient.png", img)
        exp.save_artifact("gradient.png")
        print("Saved image artifact (gradient.png).")

        # 7. Generate and save a dummy audio artifact
        from scipy.io import wavfile

        sample_rate = 44100
        t_audio = np.linspace(0, 2, sample_rate * 2)
        audio_data = np.sin(2 * np.pi * 440 * t_audio) * 0.5  # 440 Hz A note
        wavfile.write("audio.wav", sample_rate, audio_data.astype(np.float32))
        exp.save_artifact("audio.wav")
        print("Saved audio artifact (audio.wav).")

        # 8. Generate and save a dummy video artifact
        import imageio

        fps = 10
        num_frames = 20
        video_writer = imageio.get_writer("video.mp4", fps=fps)
        for i in range(num_frames):
            frame = img.copy()
            # Animate the blue channel
            frame[:, :, 2] = 0.5 + 0.5 * np.sin(10 * (x + y) + i * 2 * np.pi / num_frames)
            video_writer.append_data((frame * 255).astype(np.uint8))
        video_writer.close()
        exp.save_artifact("video.mp4")
        print("Saved video artifact (video.mp4).")

    print("Experiment finished. Start the dashboard with: expman serve ./experiments")


if __name__ == "__main__":
    main()
