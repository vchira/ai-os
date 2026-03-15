"""Voice controller — orchestrates STT and TTS for the main application."""

import threading
import logging
from typing import Callable

log = logging.getLogger(__name__)


class VoiceController:
    """High-level voice controller that bridges STT/TTS with the UI.

    Handles:
    - Continuous voice input (STT) with voice activity detection
    - Push-to-talk recording
    - TTS output of AI responses
    - Voice enable/disable toggling
    """

    def __init__(self, config_mgr):
        self.config = config_mgr
        self._stt = None
        self._tts = None
        self._audio_capture = None
        self._audio_playback = None
        self._stt_enabled = config_mgr.get("voice.stt_enabled", True)
        self._tts_enabled = config_mgr.get("voice.tts_enabled", True)
        self._on_transcription: Callable[[str], None] | None = None
        self._recording = False
        self._initialized = False

    def initialize(self):
        """Lazy-initialize voice components."""
        if self._initialized:
            return

        try:
            from aios.voice.audio import AudioCapture, AudioPlayback
            self._audio_capture = AudioCapture()
            self._audio_playback = AudioPlayback()
        except Exception as e:
            log.warning(f"Audio system unavailable: {e}")

        try:
            from aios.voice.stt import SpeechToText
            model_size = self.config.get("voice.stt_model", "medium")
            self._stt = SpeechToText(model_size=model_size)
            lang = self.config.get("voice.stt_language", "")
            if lang:
                self._stt.set_language(lang)
        except Exception as e:
            log.warning(f"STT unavailable: {e}")

        try:
            from aios.voice.tts import TextToSpeech
            voice = self.config.get("voice.tts_voice", "en_US-amy-medium")
            self._tts = TextToSpeech(voice=voice)
        except Exception as e:
            log.warning(f"TTS unavailable: {e}")

        self._initialized = True

    def set_on_transcription(self, callback: Callable[[str], None]):
        """Set callback for when STT produces a transcription."""
        self._on_transcription = callback

    def enable_stt(self):
        """Enable voice input."""
        self._stt_enabled = True
        self.config.set("voice.stt_enabled", True)

    def disable_stt(self):
        """Disable voice input."""
        self._stt_enabled = False
        self.config.set("voice.stt_enabled", False)
        self.stop_recording()

    def enable_tts(self):
        """Enable voice output."""
        self._tts_enabled = True
        self.config.set("voice.tts_enabled", True)

    def disable_tts(self):
        """Disable voice output."""
        self._tts_enabled = False
        self.config.set("voice.tts_enabled", False)
        self.stop_speaking()

    def start_recording(self):
        """Start recording audio for STT (push-to-talk)."""
        if not self._stt_enabled:
            return
        self.initialize()
        if self._audio_capture and not self._recording:
            self._recording = True
            self._audio_capture.start_recording()
            log.info("Recording started")

    def stop_recording(self):
        """Stop recording and transcribe."""
        if not self._recording or not self._audio_capture:
            return
        self._recording = False
        audio_data = self._audio_capture.stop_recording()
        log.info("Recording stopped, transcribing...")

        if audio_data is not None and len(audio_data) > 0 and self._stt:
            # Transcribe in background thread
            def do_transcribe():
                try:
                    result = self._stt.transcribe(audio_data)
                    if result.text.strip() and self._on_transcription:
                        self._on_transcription(result.text.strip())
                except Exception as e:
                    log.error(f"Transcription error: {e}")

            threading.Thread(target=do_transcribe, daemon=True).start()

    def speak(self, text: str):
        """Speak text using TTS."""
        if not self._tts_enabled:
            return
        self.initialize()
        if not self._tts or not self._audio_playback:
            return

        def do_speak():
            try:
                audio = self._tts.synthesize(text)
                if audio is not None:
                    self._audio_playback.play(audio, sample_rate=22050)
            except Exception as e:
                log.error(f"TTS error: {e}")

        threading.Thread(target=do_speak, daemon=True).start()

    def stop_speaking(self):
        """Stop current TTS playback."""
        if self._audio_playback:
            self._audio_playback.stop()

    def set_voice(self, voice_id: str):
        """Change the TTS voice."""
        self.config.set("voice.tts_voice", voice_id)
        if self._tts:
            self._tts.set_voice(voice_id)

    def set_language(self, lang: str):
        """Set STT language."""
        self.config.set("voice.stt_language", lang)
        if self._stt:
            self._stt.set_language(lang if lang else None)

    def list_voices(self) -> list[dict]:
        """List available TTS voices."""
        self.initialize()
        if self._tts:
            return [
                {
                    "id": v.id,
                    "name": v.name,
                    "language": v.language,
                    "gender": v.gender,
                    "downloaded": v.downloaded,
                }
                for v in self._tts.list_voices()
            ]
        return []

    @property
    def stt_available(self) -> bool:
        return self._stt is not None

    @property
    def tts_available(self) -> bool:
        return self._tts is not None

    @property
    def is_recording(self) -> bool:
        return self._recording
