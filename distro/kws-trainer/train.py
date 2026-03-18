#!/usr/bin/env python3
"""AiOS wake word model trainer using openWakeWord automatic training."""

import argparse
import sys
import os

def main():
    parser = argparse.ArgumentParser(description="Train a custom wake word model")
    parser.add_argument("--phrase", required=True, help="Wake word phrase to train")
    parser.add_argument("--output", required=True, help="Output .onnx model path")
    args = parser.parse_args()

    try:
        from openwakeword.train import train_model
    except ImportError:
        print("ERROR: openwakeword not installed", file=sys.stderr)
        sys.exit(1)

    print(f"Training wake word model for: {args.phrase}")
    print(f"Output: {args.output}")

    # Use openWakeWord's automatic training pipeline
    # This generates synthetic speech via Piper TTS, augments with noise,
    # and trains a small neural network
    train_model(
        target_phrase=args.phrase,
        output_path=args.output,
    )

    print(f"Training complete: {args.output}")

if __name__ == "__main__":
    main()
