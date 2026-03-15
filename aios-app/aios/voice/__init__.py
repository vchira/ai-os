"""AiOS voice subsystem -- audio capture/playback, speech-to-text, text-to-speech."""

from aios.voice.audio import AudioCapture, AudioPlayback
from aios.voice.stt import SpeechToText, TranscriptionResult, TranscriptionSegment
from aios.voice.tts import TextToSpeech, VoiceGender, VoiceInfo, VoiceQuality

__all__ = [
    "AudioCapture",
    "AudioPlayback",
    "SpeechToText",
    "TextToSpeech",
    "TranscriptionResult",
    "TranscriptionSegment",
    "VoiceGender",
    "VoiceInfo",
    "VoiceQuality",
]
