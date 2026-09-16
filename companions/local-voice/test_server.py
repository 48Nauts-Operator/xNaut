import struct
import threading
import unittest
import uuid

from fastapi.testclient import TestClient
from starlette.websockets import WebSocketDisconnect
from server import create_app, SyntheticEngine, pack_audio, unpack_audio

TOKEN = "synthetic-test-token-" + "x" * 32
AUTH = {"Authorization": "Bearer " + TOKEN}


class BridgeTests(unittest.TestCase):
    def test_auth_and_origin_are_checked_before_capabilities_or_websocket(self):
        with TestClient(create_app(TOKEN, SyntheticEngine(), synthetic=True)) as client:
            self.assertEqual(client.get("/voice/v1/capabilities").status_code, 401)
            self.assertEqual(client.get("/voice/v1/capabilities", headers={**AUTH, "Origin": "https://example.com"}).status_code, 401)
            caps = client.get("/voice/v1/capabilities", headers=AUTH).json()
            self.assertTrue(caps["synthetic"])
            self.assertFalse(caps["streaming_input"])
            self.assertEqual(caps["sample_rate"], 16000)
            with self.assertRaises(WebSocketDisconnect):
                with client.websocket_connect("/voice/v1/session"):
                    pass

    def test_supplied_text_produces_ordered_streamed_pcm_then_done(self):
        class Engine:
            def run(self, operation, payload, cancelled):
                assert operation == "synthesize"
                assert payload == "A complete answer."
                yield struct.pack("<3h", 0, -32768, 32767)
                yield struct.pack("<2h", 11, 12)
        with TestClient(create_app(TOKEN, Engine())) as client:
            with client.websocket_connect("/voice/v1/session", headers=AUTH) as ws:
                request_id = str(uuid.uuid4())
                ws.send_json({"type": "synthesize", "id": request_id, "text": "A complete answer."})
                self.assertEqual(ws.receive_json()["type"], "ready")
                self.assertEqual(unpack_audio(ws.receive_bytes(), request_id, 0), struct.pack("<3h", 0, -32768, 32767))
                self.assertEqual(unpack_audio(ws.receive_bytes(), request_id, 1), struct.pack("<2h", 11, 12))
                self.assertEqual(ws.receive_json(), {"type": "done", "id": request_id, "frames": 2})

    def test_transcription_commits_once_and_never_calls_synthesis(self):
        calls = []
        class Engine:
            def run(self, operation, payload, cancelled):
                calls.append((operation, len(payload)))
                yield {"type": "transcript.final", "text": "Please open the note."}
        with TestClient(create_app(TOKEN, Engine())) as client:
            with client.websocket_connect("/voice/v1/session", headers=AUTH) as ws:
                request_id = str(uuid.uuid4())
                ws.send_json({"type": "transcribe", "id": request_id})
                ws.send_bytes(pack_audio(request_id, 0, b"\0\0" * 4800))
                ws.send_json({"type": "input.commit", "id": request_id})
                self.assertEqual(ws.receive_json()["type"], "ready")
                self.assertEqual(ws.receive_json()["text"], "Please open the note.")
                self.assertEqual(ws.receive_json()["frames"], 1)
        self.assertEqual(calls, [("transcribe", 9600)])

    def test_stale_or_reordered_input_is_rejected_before_inference(self):
        with TestClient(create_app(TOKEN, SyntheticEngine())) as client:
            for wrong_id, sequence in [(str(uuid.uuid4()), 0), (None, 1)]:
                with client.websocket_connect("/voice/v1/session", headers=AUTH) as ws:
                    request_id = str(uuid.uuid4())
                    ws.send_json({"type": "transcribe", "id": request_id})
                    ws.send_bytes(pack_audio(wrong_id or request_id, sequence, b"\0\0" * 4800))
                    self.assertIn("out-of-order", ws.receive_json()["message"])

    def test_cancel_discards_engine_blocks_and_keeps_compute_exclusive(self):
        retired = threading.Event()
        class SlowEngine:
            def run(self, operation, payload, cancelled):
                yield b"\0\0" * 512
                cancelled.stopped.wait(5)
                # Simulate the buffered blocks observed in the real Qwen probe.
                yield b"\x01\0" * 512
                retired.set()
        with TestClient(create_app(TOKEN, SlowEngine())) as client:
            with client.websocket_connect("/voice/v1/session", headers=AUTH) as first:
                request_id = str(uuid.uuid4())
                first.send_json({"type": "synthesize", "id": request_id, "text": "first"})
                self.assertEqual(first.receive_json()["type"], "ready")
                first.receive_bytes()
                with client.websocket_connect("/voice/v1/session", headers=AUTH) as second:
                    second.send_json({"type": "synthesize", "id": str(uuid.uuid4()), "text": "second"})
                    self.assertIn("busy", second.receive_json()["message"])
                first.send_json({"type": "cancel", "id": request_id})
                with self.assertRaises(WebSocketDisconnect):
                    first.receive_bytes()
        # The generator is closed at the cancelled yield, rather than resumed.
        self.assertFalse(retired.is_set())

    def test_empty_output_and_invalid_text_fail_visibly(self):
        class EmptyEngine:
            def run(self, *args):
                return iter(())
        with TestClient(create_app(TOKEN, EmptyEngine())) as client:
            for text in ["", "界" * 1500, "valid but engine produces nothing"]:
                with client.websocket_connect("/voice/v1/session", headers=AUTH) as ws:
                    ws.send_json({"type": "synthesize", "id": str(uuid.uuid4()), "text": text})
                    event = ws.receive_json()
                    if event["type"] == "ready":
                        event = ws.receive_json()
                    self.assertEqual(event["type"], "error")


if __name__ == "__main__":
    unittest.main()
