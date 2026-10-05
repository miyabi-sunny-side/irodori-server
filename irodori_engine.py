"""Inference engine for irodori-server: an HTTP server on 0.0.0.0:IRODORI_ENGINE_PORT (7861).

POST /info, /generate and /unload take and return JSON. Requests are served one at a time, so
servers on several machines share one GPU and one loaded model. Audio crosses the boundary as
base64; the engine keeps it in a temporary directory it names itself and removes after the
reply. Output written while a request runs is returned as `engine_log` and kept on stderr.
The server writes every database row and final file.
"""
from __future__ import annotations

import base64
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import traceback

os.environ.setdefault("GRADIO_ANALYTICS_ENABLED", "False")
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
os.environ.setdefault("DO_NOT_TRACK", "1")
os.environ.setdefault("WANDB_MODE", "disabled")

ROOT = Path(__file__).resolve().parent
REPO = ROOT / "Irodori-TTS"
FORMAT = re.compile(r"[a-z0-9]{1,8}")


def load_upstream():
    import importlib.util
    sys.path.insert(0, str(REPO))
    spec = importlib.util.spec_from_file_location("irodori_upstream", REPO / "gradio_app_voicedesign.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def decode_references(items):
    """[{format, data(base64)}] -> [(extension, bytes)]; the extension becomes part of a file name."""
    references = []
    for item in items or []:
        fmt = str(item.get("format", "")).lower()
        if not FORMAT.fullmatch(fmt):
            raise ValueError(f"unsupported reference format: {fmt!r}")
        references.append((fmt, base64.b64decode(item.get("data", ""), validate=True)))
    return references


def info(upstream, _body):
    from irodori_tts.gradio_emoji_palette import EMOJI_PALETTE_ITEMS
    devices = upstream.list_available_runtime_devices()
    return {
        "devices": devices,
        "precisions": {device: upstream._precision_choices_for_device(device) for device in devices},
        "max_candidates": upstream.MAX_GRADIO_CANDIDATES,
        "emojis": [{"emoji": i.emoji, "label": i.label, "description": i.description} for i in EMOJI_PALETTE_ITEMS],
    }


def generate(upstream, body):
    references = decode_references(body.get("references"))
    params = dict(body["params"])
    count = upstream.MAX_GRADIO_CANDIDATES
    with tempfile.TemporaryDirectory(prefix="irodori-engine-") as work:
        paths = []
        for index, (fmt, data) in enumerate(references):
            path = Path(work, f"ref_{index:02d}.{fmt}")
            path.write_bytes(data)
            paths.append(str(path))
        params["ref_wavs"] = paths or None
        # Upstream saves into "gradio_outputs_voicedesign" under the working directory.
        os.chdir(work)
        try:
            result = upstream._run_generation(**params)
        finally:
            os.chdir(ROOT)
        wavs = [base64.b64encode(Path(work, item["value"]).read_bytes()).decode()
                for item in result[:count] if item.get("value")]
    detail, timing = result[count:]
    seed = next((line.split(":", 1)[1].strip() for line in detail.splitlines() if line.startswith("seed_used:")), None)
    return {"wavs": wavs, "log": f"{detail}\n\n{timing}", "seed": seed}


def unload(upstream, _body):
    upstream._clear_runtime_cache()
    return {}


OPS = {"info": info, "generate": generate, "unload": unload}


def run_captured(call):
    """Runs call() with fds 1 and 2 captured; returns its reply with ok and engine_log."""
    sys.stdout.flush()
    sys.stderr.flush()
    saved = os.dup(1), os.dup(2)
    with tempfile.TemporaryFile() as capture:
        os.dup2(capture.fileno(), 1)
        os.dup2(capture.fileno(), 2)
        try:
            reply = call()
            reply["ok"] = True
        except Exception as exc:  # reported to the server, which maps it to a user message
            traceback.print_exc()
            reply = {"ok": False, "error_type": type(exc).__name__, "error": str(exc),
                     "trace": traceback.format_exc()}
        finally:
            sys.stdout.flush()
            sys.stderr.flush()
            for fd, original in zip((1, 2), saved):
                os.dup2(original, fd)
                os.close(original)
        capture.seek(0)
        log = capture.read().decode("utf-8", "replace")
    sys.stderr.write(log)
    sys.stderr.flush()
    reply["engine_log"] = log
    return reply


class Handler(BaseHTTPRequestHandler):
    upstream = None

    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get("Content-Length") or 0))

        def call():
            op = OPS.get(self.path.strip("/"))
            if op is None:
                raise ValueError(f"unknown op: {self.path}")
            return op(self.upstream, json.loads(raw or b"{}"))

        reply = run_captured(call)
        data = json.dumps(reply, ensure_ascii=False).encode()
        self.send_response(200 if reply["ok"] else 500)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def check(backend):
    """Dependency and device check run by irodori.sh before the engine uses this environment."""
    import soundfile  # noqa: F401
    import torch
    import torchaudio  # noqa: F401
    import torchcodec  # noqa: F401
    if backend == "cu128" and not torch.cuda.is_available():
        raise SystemExit("CUDA is unavailable. Check the NVIDIA driver or rerun with --backend cpu.")
    if backend == "xpu" and not torch.xpu.is_available():
        raise SystemExit("Intel XPU is unavailable. Check the driver or rerun with --backend cpu.")
    load_upstream()
    print(f"Dependency check passed. Python: {sys.executable}; PyTorch: {torch.__version__}")


def main():
    if sys.argv[1:2] == ["--check"]:
        check(sys.argv[2] if len(sys.argv) > 2 else "cpu")
        return
    port = int(os.environ.get("IRODORI_ENGINE_PORT", "7861"))
    Handler.upstream = load_upstream()
    server = HTTPServer(("0.0.0.0", port), Handler)
    print(f"irodori engine listening on 0.0.0.0:{port}", file=sys.stderr, flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
