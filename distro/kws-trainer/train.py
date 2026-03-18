#!/usr/bin/env python3
"""AiOS wake word model trainer using openWakeWord automatic training.

This script wraps the openWakeWord training pipeline to train custom wake
word models from a text phrase. It:
1. Generates a YAML training config for the target phrase
2. Generates synthetic speech samples via Piper TTS
3. Augments with noise and room impulse responses
4. Trains a small neural network
5. Exports to ONNX format

Requirements:
  - openWakeWord repo cloned at /opt/aios-app/kws-trainer/openwakeword/
  - Python packages: torch, speechbrain, onnxruntime, piper-sample-generator
  - Piper TTS models (shipped with AiOS)

Usage:
  python train.py --phrase "hey assistant" --output /path/to/model.onnx
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import yaml


OPENWAKEWORD_DIR = os.environ.get(
    "OPENWAKEWORD_DIR",
    "/opt/aios-app/kws-trainer/openwakeword",
)
TRAIN_SCRIPT = os.path.join(OPENWAKEWORD_DIR, "openwakeword", "train.py")
BASE_CONFIG = os.path.join(OPENWAKEWORD_DIR, "examples", "custom_model.yml")


def check_environment():
    """Verify the training environment is available."""
    if not os.path.isfile(TRAIN_SCRIPT):
        print(f"ERROR: openWakeWord train.py not found at {TRAIN_SCRIPT}", file=sys.stderr)
        print("Clone the repo: git clone https://github.com/dscripka/openWakeWord.git", file=sys.stderr)
        sys.exit(1)

    try:
        import torch  # noqa: F401
        import speechbrain  # noqa: F401
        import onnxruntime  # noqa: F401
    except ImportError as e:
        print(f"ERROR: missing dependency: {e}", file=sys.stderr)
        sys.exit(1)


def create_training_config(phrase: str, work_dir: str) -> str:
    """Create a YAML training config for the given phrase."""
    model_name = phrase.lower().replace(" ", "_").replace(
        lambda c: c if c.isalnum() or c == "_" else "", ""
    )

    # Start from base config if available, otherwise create minimal config
    if os.path.isfile(BASE_CONFIG):
        with open(BASE_CONFIG) as f:
            config = yaml.safe_load(f)
    else:
        config = {}

    config.update({
        "target_phrase": [phrase],
        "model_name": model_name,
        "n_samples": 3000,
        "n_samples_val": 500,
        "steps": 50000,
        "target_accuracy": [0.5],
        "target_recall": [0.5],
        "output_dir": work_dir,
    })

    config_path = os.path.join(work_dir, "training_config.yaml")
    with open(config_path, "w") as f:
        yaml.dump(config, f, default_flow_style=False)

    return config_path


def run_training(config_path: str):
    """Run the three-step openWakeWord training pipeline."""
    python = sys.executable

    steps = [
        ("Generating synthetic speech clips...", ["--generate_clips"]),
        ("Augmenting clips with noise...", ["--augment_clips"]),
        ("Training model...", ["--train_model"]),
    ]

    for description, args in steps:
        print(f"\n{'='*60}")
        print(f"  {description}")
        print(f"{'='*60}\n")

        cmd = [python, TRAIN_SCRIPT, "--training_config", config_path] + args
        result = subprocess.run(cmd, cwd=OPENWAKEWORD_DIR)

        if result.returncode != 0:
            print(f"ERROR: Training step failed: {description}", file=sys.stderr)
            sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        description="Train a custom wake word model for AiOS"
    )
    parser.add_argument("--phrase", required=True, help="Wake word phrase to train")
    parser.add_argument("--output", required=True, help="Output .onnx model path")
    args = parser.parse_args()

    check_environment()

    print(f"Training wake word model for: \"{args.phrase}\"")
    print(f"Output: {args.output}")

    with tempfile.TemporaryDirectory(prefix="aios-kws-train-") as work_dir:
        # Create training config
        config_path = create_training_config(args.phrase, work_dir)
        print(f"Training config: {config_path}")

        # Run the training pipeline
        run_training(config_path)

        # Find and copy the output model
        model_name = args.phrase.lower().replace(" ", "_")
        candidates = [
            os.path.join(work_dir, f"{model_name}.onnx"),
            os.path.join(work_dir, "output", f"{model_name}.onnx"),
        ]

        # Also search work_dir recursively for any .onnx file
        for root, dirs, files in os.walk(work_dir):
            for f in files:
                if f.endswith(".onnx"):
                    candidates.append(os.path.join(root, f))

        model_found = None
        for candidate in candidates:
            if os.path.isfile(candidate):
                model_found = candidate
                break

        if model_found is None:
            print("ERROR: Training completed but no .onnx model found", file=sys.stderr)
            sys.exit(1)

        # Copy to output path
        os.makedirs(os.path.dirname(os.path.abspath(args.output)), exist_ok=True)
        shutil.copy2(model_found, args.output)

    print(f"\nTraining complete: {args.output}")
    print(f"Model size: {os.path.getsize(args.output) / 1024:.0f} KB")


if __name__ == "__main__":
    main()
