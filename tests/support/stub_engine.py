"""Stand-in for irodori_engine.py in tests: same HTTP contract, no model, tiny WAV files.

Listens on a free port printed as the first stdout line, and exits when stdin closes.
"""
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import sys
import threading
import wave

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from irodori_engine import decode_references  # noqa: E402


def tiny_wav():
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(8000)
        audio.writeframes(b"\0\0" * 800)
    return base64.b64encode(buffer.getvalue()).decode()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        op = self.path.strip("/")
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length") or 0)) or b"{}")
        reply = {"ok": True, "engine_log": f"stub {op}"}
        if op == "info":
            reply |= {"devices": ["cpu"], "precisions": {"cpu": ["fp32"]}, "max_candidates": 4,
                      "emojis": [{"emoji": "😊", "label": "楽しげ", "description": "楽しそうに"}]}
        elif op == "fail":
            reply |= {"ok": False, "error_type": "ValueError", "error": "bad", "trace": "Traceback: bad"}
        elif op == "generate":
            params = body["params"]
            if params["text"] == "落ちる":
                return  # the connection closes without a reply, as when the engine dies
            elif params["text"] == "失敗":
                reply |= {"ok": False, "error_type": "ValueError", "error": "seed must be an integer",
                          "trace": "Traceback: seed"}
            else:
                references = [(fmt, data.decode("utf-8", "replace")) for fmt, data in decode_references(body.get("references"))]
                reply |= {"wavs": [tiny_wav() for _ in range(int(params["num_candidates"]))], "seed": "42",
                          "log": "stub\n" + json.dumps(params, ensure_ascii=False)
                          + "\nreferences: " + json.dumps(references)}
        data = json.dumps(reply, ensure_ascii=False).encode()
        self.send_response(200 if reply["ok"] else 500)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
threading.Thread(target=server.serve_forever, daemon=True).start()
sys.stdin.read()
os._exit(0)
