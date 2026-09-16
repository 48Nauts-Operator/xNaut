"""Optional Xnaut speech-only companion (protocol v1); never loads a chat LLM.

Engine API consumer of speech-to-speech (Apache-2.0), revision 0355f39e:
STT/parakeet_tdt_handler.py::process, TTS/qwen3_tts_handler.py::process.
Unlike its conversation server, this adapter sends supplied text directly to
TTS. Authentication, bounded framing and transport cancellation are Xnaut code.
"""
import argparse
import asyncio
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FutureTimeout
import contextlib
import hmac
import json
import os
from pathlib import Path
from queue import Queue
import secrets
import struct
from threading import Event
import uuid

from fastapi import FastAPI, HTTPException, Request, WebSocket, WebSocketDisconnect

RATE = 16000
MAX_FRAME = 16384
MAX_AUDIO = RATE * 2 * 120
CAPABILITIES = {
    "version": 1, "recognition": True, "synthesis": True,
    "streaming_input": False, "streaming_output": True,
    "format": "pcm_s16le", "sample_rate": RATE, "channels": 1,
    "max_text_bytes": 4096, "max_input_seconds": 120,
}


def pack_audio(request_id, sequence, pcm):
    if not pcm or len(pcm) % 2 or len(pcm) > MAX_FRAME:
        raise ValueError("invalid PCM frame")
    return uuid.UUID(request_id).bytes + struct.pack(">I", sequence) + pcm


def unpack_audio(frame, request_id, sequence):
    if len(frame) < 22 or len(frame) > MAX_FRAME + 20 or (len(frame) - 20) % 2:
        raise ValueError("invalid PCM frame")
    if frame[:16] != uuid.UUID(request_id).bytes or struct.unpack(">I", frame[16:20])[0] != sequence:
        raise ValueError("stale or out-of-order PCM frame")
    return frame[20:]


class Cancellation:
    def __init__(self):
        self.stopped = Event()
        self.scope = None

    def cancel(self):
        self.stopped.set()
        if self.scope is not None:
            self.scope.cancel()


class SpeechEngines:
    """Created/used on one dedicated thread; never instantiate the LLM pipeline."""
    def __init__(self, args):
        self.args = args
        self.stt = self.tts = None

    def run(self, operation, payload, cancelled):
        # The upstream handlers print utterances. Keep conversation text out of
        # companion stdout. Access logs are disabled as well.
        with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink):
            yield from self._run(operation, payload, cancelled)

    def _run(self, operation, payload, cancelled):
        import numpy as np
        from speech_to_speech.pipeline.messages import TTSInput, VADAudio
        if operation == "transcribe":
            if self.stt is None:
                from speech_to_speech.STT.parakeet_tdt_handler import ParakeetTDTSTTHandler
                self.stt = ParakeetTDTSTTHandler(Event(), Queue(), Queue(), setup_kwargs={
                    "model_name": self.args.stt_model, "device": "mps",
                })
            if cancelled.stopped.is_set():
                return
            audio = np.frombuffer(payload, dtype="<i2").astype(np.float32) / 32768
            for item in self.stt.process(VADAudio(audio=audio, mode="final")):
                if item.tag == "transcription" and not cancelled.stopped.is_set():
                    yield {"type": "transcript.final", "text": item.text}
        else:
            if self.tts is None:
                from speech_to_speech.TTS.qwen3_tts_handler import Qwen3TTSHandler
                from speech_to_speech.pipeline.cancel_scope import CancelScope
                self.tts = Qwen3TTSHandler(Event(), Queue(), Queue(), setup_kwargs={
                    "should_listen": Event(), "model_name": self.args.tts_model,
                    "device": "mps", "ref_audio": self.args.reference,
                    "ref_text": self.args.reference_text, "streaming_chunk_size": 8,
                    "cancel_scope": CancelScope(),
                })
            cancelled.scope = self.tts.cancel_scope
            cancelled.scope.new_response()
            if cancelled.stopped.is_set():
                return
            for block in self.tts.process(TTSInput(text=payload)):
                # Qwen can yield buffered blocks after scope.cancel(). Guard
                # every block here, not only the outer generation iterator.
                if cancelled.stopped.is_set():
                    break
                pcm = block if isinstance(block, bytes) else np.asarray(block, dtype="<i2").tobytes()
                for offset in range(0, len(pcm), MAX_FRAME):
                    if cancelled.stopped.is_set():
                        return
                    yield pcm[offset:offset + MAX_FRAME]


class SyntheticEngine:
    """Explicit test mode; never advertised as actual speech recognition."""
    def run(self, operation, payload, cancelled):
        if operation == "transcribe":
            yield {"type": "transcript.final", "text": "Synthetic transcript"}
        else:
            for _ in range(10):
                if cancelled.stopped.is_set():
                    return
                yield struct.pack("<512h", *([0] * 512))


def create_app(token, engine, *, synthetic=False):
    if len(token) < 32:
        raise ValueError("a token of at least 32 characters is required")
    busy = asyncio.Lock()
    executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="xnaut-speech")
    @contextlib.asynccontextmanager
    async def lifespan(_app):
        try:
            yield
        finally:
            executor.shutdown(wait=False, cancel_futures=True)
    app = FastAPI(docs_url=None, redoc_url=None, openapi_url=None, lifespan=lifespan)

    def authorized(headers):
        return not headers.get("origin") and hmac.compare_digest(headers.get("authorization", ""), "Bearer " + token)

    @app.get("/voice/v1/capabilities")
    async def capabilities(request: Request):
        if not authorized(request.headers):
            raise HTTPException(401, "unauthorized")
        return {**CAPABILITIES, "synthetic": synthetic}

    @app.websocket("/voice/v1/session")
    async def session(ws: WebSocket):
        if not authorized(ws.headers):
            await ws.close(code=1008)
            return
        await ws.accept()
        cancelled = Cancellation()
        worker = watcher = None
        request_id = None
        acquired = False
        try:
            first = await asyncio.wait_for(ws.receive_text(), 10)
            if len(first.encode()) > 8192:
                raise ValueError("request too large")
            request = json.loads(first)
            request_id = str(uuid.UUID(request["id"]))
            operation = request["type"]
            if busy.locked():
                raise ValueError("speech engine busy; try again after the current utterance")
            await busy.acquire()
            acquired = True
            if operation == "transcribe":
                audio = bytearray()
                sequence = 0
                upload_deadline = asyncio.get_running_loop().time() + 15
                while True:
                    remaining = upload_deadline - asyncio.get_running_loop().time()
                    if remaining <= 0:
                        raise ValueError("audio upload timed out")
                    message = await asyncio.wait_for(ws.receive(), remaining)
                    if message["type"] == "websocket.disconnect":
                        raise WebSocketDisconnect()
                    if message.get("bytes") is not None:
                        audio.extend(unpack_audio(message["bytes"], request_id, sequence))
                        sequence += 1
                        if len(audio) > MAX_AUDIO:
                            raise ValueError("recording exceeds 120 seconds")
                    else:
                        raw = message.get("text", "")
                        if len(raw) > 1024:
                            raise ValueError("control frame too large")
                        commit = json.loads(raw)
                        if commit.get("type") != "input.commit" or commit.get("id") != request_id:
                            raise ValueError("expected input.commit for this utterance")
                        break
                if len(audio) < RATE * 2 * 0.3:
                    raise ValueError("recording is too short")
                payload = bytes(audio)
            elif operation == "synthesize":
                payload = request.get("text")
                if not isinstance(payload, str) or not payload.strip() or len(payload.encode()) > CAPABILITIES["max_text_bytes"]:
                    raise ValueError("speech text must be nonempty and at most 4096 bytes")
            else:
                raise ValueError("unsupported operation")

            await ws.send_json({"type": "ready", "id": request_id, **CAPABILITIES})
            loop = asyncio.get_running_loop()
            outgoing = asyncio.Queue(maxsize=8)

            def put(item):
                future = asyncio.run_coroutine_threadsafe(outgoing.put(item), loop)
                while not cancelled.stopped.is_set():
                    try:
                        future.result(timeout=0.05)
                        return True
                    except FutureTimeout:
                        continue
                future.cancel()
                return False

            def produce():
                stream = None
                try:
                    stream = engine.run(operation, payload, cancelled)
                    for item in stream:
                        if cancelled.stopped.is_set() or not put(item):
                            return
                    put(None)
                except Exception:
                    # Avoid leaking model paths, tokens or utterances through errors.
                    put({"type": "error", "message": "local speech engine failed; check its configuration and cached models"})
                finally:
                    close = getattr(stream, "close", None)
                    if close:
                        close()

            async def watch_cancel():
                try:
                    message = await ws.receive()
                    if message["type"] != "websocket.disconnect":
                        raw = message.get("text", "")
                        if len(raw) > 1024:
                            raise ValueError("control frame too large")
                        event = json.loads(raw)
                        if event.get("type") != "cancel" or event.get("id") != request_id:
                            raise ValueError("expected cancel")
                finally:
                    cancelled.cancel()

            worker = loop.run_in_executor(executor, produce)
            watcher = asyncio.create_task(watch_cancel())
            sequence = 0
            while not cancelled.stopped.is_set():
                get = asyncio.create_task(outgoing.get())
                try:
                    done, _ = await asyncio.wait([get, watcher], timeout=180, return_when=asyncio.FIRST_COMPLETED)
                    if not done:
                        raise ValueError("speech engine timed out")
                    if watcher in done:
                        break
                    item = get.result()
                finally:
                    if not get.done():
                        get.cancel()
                        with contextlib.suppress(asyncio.CancelledError):
                            await get
                if cancelled.stopped.is_set():
                    break
                if item is None:
                    if not sequence:
                        raise ValueError("speech engine produced no output")
                    await ws.send_json({"type": "done", "id": request_id, "frames": sequence})
                    break
                if isinstance(item, bytes):
                    await asyncio.wait_for(ws.send_bytes(pack_audio(request_id, sequence, item)), 10)
                else:
                    await ws.send_json({**item, "id": request_id})
                    if item["type"] == "error":
                        break
                sequence += 1
        except (WebSocketDisconnect, RuntimeError):
            pass
        except (ValueError, KeyError, TypeError, asyncio.TimeoutError) as error:
            with contextlib.suppress(Exception):
                await ws.send_json({"type": "error", "id": request_id, "message": str(error)[:160]})
        finally:
            cancelled.cancel()
            if watcher:
                watcher.cancel()
                with contextlib.suppress(Exception, asyncio.CancelledError):
                    await watcher
            # Keep ownership while a cancelled model is still unwinding. No
            # second client may call the same MLX object concurrently.
            if worker:
                with contextlib.suppress(Exception, asyncio.CancelledError):
                    await asyncio.shield(worker)
            if acquired:
                busy.release()
            with contextlib.suppress(Exception):
                await ws.close()

    return app


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8791)
    parser.add_argument("--connection-file", type=Path, required=True)
    parser.add_argument("--stt-model")
    parser.add_argument("--tts-model")
    parser.add_argument("--reference")
    parser.add_argument("--reference-text", default="")
    parser.add_argument("--synthetic", action="store_true", help="test-only silent PCM and fixed transcript")
    args = parser.parse_args()
    if not 1024 <= args.port <= 65535:
        parser.error("port must be between 1024 and 65535")
    if not args.synthetic and not (args.stt_model and args.tts_model):
        parser.error("real speech requires explicit cached --stt-model and --tts-model")
    os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1")
    # A separate private profile holds only endpoint + token; it is never sent
    # to the webview. Reuse it across restarts without overwriting other keys.
    if args.connection_file.exists():
        config = json.loads(args.connection_file.read_text())
        if config.get("endpoint") != f"http://127.0.0.1:{args.port}":
            parser.error("connection file endpoint does not match --port")
        token = config["token"]
    else:
        token = secrets.token_urlsafe(32)
        args.connection_file.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(args.connection_file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as f:
            json.dump({"endpoint": f"http://127.0.0.1:{args.port}", "token": token}, f)
    import uvicorn
    app = create_app(token, SyntheticEngine() if args.synthetic else SpeechEngines(args), synthetic=args.synthetic)
    uvicorn.run(app, host="127.0.0.1", port=args.port, access_log=False,
                ws_max_size=MAX_FRAME + 20, ws_max_queue=8)


if __name__ == "__main__":
    main()
