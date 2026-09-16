#!/usr/bin/env python3
"""Opt-in speech-only probe; run with the optional runtime's Python environment.

Uses speech-to-speech (Apache-2.0), ParakeetTDTSTTHandler.process and
Qwen3TTSHandler.process at revision 0355f39e. This is an API consumer, not a
pipeline port: no conversational LLM, server, microphone or speaker is started.
Only cached models are allowed. Supply synthetic/authorized audio explicitly.
"""
import argparse
import json
import os
from queue import Queue
from threading import Event
from time import perf_counter


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine", choices=["stt", "tts"])
    parser.add_argument("--model", required=True)
    parser.add_argument("--input", help="16 kHz mono synthetic WAV for STT")
    parser.add_argument("--reference", help="Synthetic/authorized voice reference for a Base TTS model")
    parser.add_argument("--reference-text", default="")
    parser.add_argument("--text", default="The written answer stays complete. This is the spoken summary.")
    parser.add_argument("--cancel-after-blocks", type=int, default=0)
    args = parser.parse_args()
    if args.engine == "stt" and not args.input:
        parser.error("STT needs --input")
    if args.cancel_after_blocks < 0:
        parser.error("--cancel-after-blocks must be non-negative")

    # Set before importing libraries: the probe must not fetch models or send
    # utterances to a cloud service when the local cache is incomplete.
    os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1")
    import numpy as np
    from speech_to_speech.pipeline.messages import TTSInput, VADAudio

    began = perf_counter()
    stop = Event()
    if args.engine == "stt":
        import soundfile as sf
        from speech_to_speech.STT.parakeet_tdt_handler import ParakeetTDTSTTHandler
        audio, rate = sf.read(args.input, dtype="float32")
        if rate != 16000 or audio.ndim != 1 or len(audio) > 16000 * 30:
            parser.error("input must be mono 16 kHz, at most 30 seconds")
        handler = ParakeetTDTSTTHandler(stop, Queue(), Queue(), setup_kwargs={
            "model_name": args.model, "device": "mps", "enable_live_transcription": True,
        })
    else:
        from speech_to_speech.TTS.qwen3_tts_handler import Qwen3TTSHandler
        from speech_to_speech.pipeline.cancel_scope import CancelScope
        scope = CancelScope()
        handler = Qwen3TTSHandler(stop, Queue(), Queue(), setup_kwargs={
            "should_listen": Event(), "model_name": args.model, "device": "mps",
            "ref_audio": args.reference, "ref_text": args.reference_text,
            "cancel_scope": scope, "streaming_chunk_size": 8,
        })
    load_seconds = perf_counter() - began
    try:
        runs = []
        for turn in range(2):
            began = perf_counter()
            if args.engine == "stt":
                results = list(handler.process(VADAudio(audio=audio, mode="final", turn_id=str(turn), turn_revision=0)))
                text = " ".join(item.text for item in results if item.tag == "transcription")
                if not text.strip():
                    raise RuntimeError("recognizer returned no final text")
                runs.append({"elapsed_seconds": perf_counter() - began, "text": text})
            else:
                scope.new_response()
                first = None
                count = samples = late_blocks = 0
                cancelled_at = None
                stream = handler.process(TTSInput(text=args.text, turn_id=str(turn), turn_revision=0))
                try:
                    for block in stream:
                        if isinstance(block, bytes):
                            block = np.frombuffer(block, dtype=np.int16)
                        if not len(block):
                            continue
                        if first is None:
                            first = perf_counter() - began
                        count += 1
                        samples += len(block)
                        if cancelled_at is not None:
                            # Diagnostic only. A real renderer must discard these.
                            late_blocks += 1
                        if args.cancel_after_blocks and count == args.cancel_after_blocks:
                            cancelled_at = perf_counter()
                            scope.cancel()
                        if perf_counter() - began > 120:
                            raise RuntimeError("TTS exceeded the probe's 120-second processing budget")
                finally:
                    stream.close()
                if not samples:
                    raise RuntimeError("TTS produced no PCM blocks")
                runs.append({
                    "first_pcm_seconds": first, "elapsed_seconds": perf_counter() - began,
                    "pcm_blocks": count, "audio_seconds": samples / 16000,
                    "blocks_after_cancel": late_blocks,
                    "cancel_to_return_seconds": None if cancelled_at is None else perf_counter() - cancelled_at,
                })
        print("VOICE_PROBE " + json.dumps({
            "engine": args.engine, "model": args.model, "load_and_warmup_seconds": load_seconds,
            "runs": runs, "conversational_llm_created": False, "speaker_playback": False,
        }))
    finally:
        stop.set()
        handler.cleanup()


if __name__ == "__main__":
    main()
