"""Speech-to-Text engine for AiOS using faster-whisper.

Converts audio (NumPy arrays at 16 kHz mono float32) into text via
OpenAI Whisper models running locally through the ``faster-whisper``
library (CTranslate2 backend).
"""

from __future__ import annotations

import logging
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

import numpy as np

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MODELS_DIR = Path(os.environ.get("AIOS_MODELS_DIR", Path.home() / ".aios" / "models" / "whisper"))

AVAILABLE_MODELS: list[str] = [
    "tiny",
    "base",
    "small",
    "medium",
    "large-v3",
]


# ---------------------------------------------------------------------------
# Data types
# ---------------------------------------------------------------------------


@dataclass
class TranscriptionSegment:
    """A single timed segment from the transcription.

    Attributes:
        start: Start time in seconds.
        end: End time in seconds.
        text: Transcribed text for this segment.
        confidence: Average log-probability (higher is more confident).
    """

    start: float
    end: float
    text: str
    confidence: float


@dataclass
class TranscriptionResult:
    """Full result of a speech-to-text transcription.

    Attributes:
        text: Complete transcribed text.
        language: Detected (or forced) language code (e.g. ``"en"``).
        confidence: Average confidence across all segments (0.0 -- 1.0).
        segments: Per-segment detail with timestamps.
    """

    text: str
    language: str
    confidence: float
    segments: list[TranscriptionSegment] = field(default_factory=list)


# ---------------------------------------------------------------------------
# SpeechToText
# ---------------------------------------------------------------------------


class SpeechToText:
    """Transcribe audio using faster-whisper (local Whisper inference).

    The model is loaded lazily on first call to :meth:`transcribe` (or
    explicitly via :meth:`load_model`).  Model files are stored under
    ``~/.aios/models/whisper/<model_size>/``.

    Parameters:
        model_size: One of :data:`AVAILABLE_MODELS`.
        device: ``"cpu"`` or ``"cuda"`` (GPU requires compatible CTranslate2).
        compute_type: Quantisation level -- ``"int8"``, ``"float16"``, or
            ``"float32"``.
    """

    available_models: list[str] = AVAILABLE_MODELS

    def __init__(
        self,
        model_size: str = "medium",
        device: str = "cpu",
        compute_type: str = "int8",
    ) -> None:
        if model_size not in AVAILABLE_MODELS:
            raise ValueError(
                f"Unknown model size {model_size!r}. "
                f"Available: {', '.join(AVAILABLE_MODELS)}"
            )

        self.model_size = model_size
        self.device = device
        self.compute_type = compute_type
        self._language: Optional[str] = None
        self._model: object | None = None

    # -- public API ---------------------------------------------------------

    @property
    def is_loaded(self) -> bool:
        """``True`` if the underlying Whisper model has been loaded."""
        return self._model is not None

    def set_language(self, lang: Optional[str]) -> None:
        """Set the preferred transcription language.

        Parameters:
            lang: An ISO-639-1 language code (e.g. ``"en"``, ``"de"``).
                Pass ``None`` to re-enable auto-detection.
        """
        self._language = lang
        logger.info("STT language set to %s", lang or "auto-detect")

    def load_model(self) -> None:
        """Load the Whisper model into memory.

        This is called automatically on the first :meth:`transcribe` call,
        but can be invoked manually to control when the (potentially large)
        model load occurs.

        Raises:
            RuntimeError: If ``faster-whisper`` is not installed or the model
                cannot be loaded.
        """
        if self._model is not None:
            return

        try:
            from faster_whisper import WhisperModel
        except ImportError as exc:
            raise RuntimeError(
                "faster-whisper is required for speech-to-text. "
                "Install it with: pip install faster-whisper"
            ) from exc

        model_dir = MODELS_DIR / self.model_size
        MODELS_DIR.mkdir(parents=True, exist_ok=True)

        # faster-whisper will download the model automatically from HuggingFace
        # if not already cached.  We point download_root at our own directory
        # so all models live under ~/.aios/models/whisper/.
        logger.info(
            "Loading Whisper model %s (device=%s, compute_type=%s) ...",
            self.model_size,
            self.device,
            self.compute_type,
        )

        try:
            self._model = WhisperModel(
                self.model_size,
                device=self.device,
                compute_type=self.compute_type,
                download_root=str(MODELS_DIR),
            )
        except Exception as exc:
            raise RuntimeError(
                f"Failed to load Whisper model '{self.model_size}': {exc}"
            ) from exc

        logger.info("Whisper model %s loaded successfully", self.model_size)

    def transcribe(
        self,
        audio: np.ndarray,
        language: Optional[str] = None,
    ) -> TranscriptionResult:
        """Transcribe an audio array to text.

        Parameters:
            audio: Audio samples as a 1-D ``float32`` NumPy array at 16 kHz.
                If the array is 2-D (stereo), only the first channel is used.
            language: Override language for this call.  ``None`` uses the
                language set via :meth:`set_language` (or auto-detect).

        Returns:
            A :class:`TranscriptionResult` with full text and per-segment
            detail.

        Raises:
            RuntimeError: If the model is not loaded and cannot be loaded.
            ValueError: If *audio* is empty.
        """
        if audio.size == 0:
            raise ValueError("Cannot transcribe empty audio")

        # Ensure mono float32
        if audio.ndim == 2:
            audio = audio[:, 0]
        audio = audio.astype(np.float32)

        # Lazy-load model
        if self._model is None:
            self.load_model()

        lang = language or self._language

        logger.debug(
            "Transcribing %.2f s of audio (language=%s)",
            len(audio) / 16000,
            lang or "auto",
        )

        try:
            segments_iter, info = self._model.transcribe(  # type: ignore[union-attr]
                audio,
                language=lang,
                beam_size=5,
                vad_filter=True,
                vad_parameters=dict(
                    min_silence_duration_ms=500,
                    speech_pad_ms=200,
                ),
            )
        except Exception as exc:
            raise RuntimeError(f"Transcription failed: {exc}") from exc

        segments: list[TranscriptionSegment] = []
        full_text_parts: list[str] = []
        total_confidence = 0.0

        for seg in segments_iter:
            # avg_logprob is in log space; convert to a 0-1 probability
            prob = _logprob_to_confidence(seg.avg_logprob)
            segments.append(
                TranscriptionSegment(
                    start=seg.start,
                    end=seg.end,
                    text=seg.text.strip(),
                    confidence=prob,
                )
            )
            full_text_parts.append(seg.text.strip())
            total_confidence += prob

        avg_confidence = total_confidence / max(len(segments), 1)
        full_text = " ".join(full_text_parts)

        detected_language = info.language if info else (lang or "en")

        result = TranscriptionResult(
            text=full_text,
            language=detected_language,
            confidence=round(avg_confidence, 4),
            segments=segments,
        )

        logger.info(
            "Transcription complete: %d chars, lang=%s, confidence=%.2f",
            len(result.text),
            result.language,
            result.confidence,
        )
        return result

    @staticmethod
    def download_model(model_size: str) -> Path:
        """Download a Whisper model if it is not already cached.

        Parameters:
            model_size: One of :data:`AVAILABLE_MODELS`.

        Returns:
            Path to the local model directory.

        Raises:
            ValueError: If *model_size* is not recognised.
            RuntimeError: If the download or conversion fails.
        """
        if model_size not in AVAILABLE_MODELS:
            raise ValueError(
                f"Unknown model size {model_size!r}. "
                f"Available: {', '.join(AVAILABLE_MODELS)}"
            )

        try:
            from faster_whisper import WhisperModel
        except ImportError as exc:
            raise RuntimeError(
                "faster-whisper is required to download models. "
                "Install it with: pip install faster-whisper"
            ) from exc

        MODELS_DIR.mkdir(parents=True, exist_ok=True)

        logger.info("Downloading Whisper model '%s' ...", model_size)
        try:
            # Constructing the model triggers the download
            WhisperModel(
                model_size,
                device="cpu",
                compute_type="int8",
                download_root=str(MODELS_DIR),
            )
        except Exception as exc:
            raise RuntimeError(
                f"Failed to download model '{model_size}': {exc}"
            ) from exc

        model_path = MODELS_DIR / model_size
        logger.info("Model '%s' ready at %s", model_size, model_path)
        return model_path

    def unload_model(self) -> None:
        """Release the loaded model from memory."""
        self._model = None
        logger.info("Whisper model unloaded")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _logprob_to_confidence(avg_logprob: float) -> float:
    """Convert Whisper's average log-probability to a 0--1 confidence.

    Whisper log-probs are typically in the range [-1, 0].  We clamp and
    normalise so the result is intuitive for callers.
    """
    import math

    # exp(log_prob) gives a probability, but avg_logprob can be quite
    # negative for uncertain segments.  Clamp to a sensible floor.
    clamped = max(avg_logprob, -1.0)
    return round(math.exp(clamped), 4)
