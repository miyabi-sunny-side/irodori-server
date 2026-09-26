"""Inference worker owned by irodori-server: one JSON request per stdin line, one reply per line.

The protocol uses a private copy of stdout; upstream prints its logs to stdout, so fd 1 is
redirected to stderr. The server writes every database row and final file; this process only
generates WAVs into the directory it is given.
"""
from __future__ import annotations

import dataclasses
import json
import os
from pathlib import Path
import sys
import traceback

os.environ.setdefault("GRADIO_ANALYTICS_ENABLED", "False")
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
os.environ.setdefault("DO_NOT_TRACK", "1")
os.environ.setdefault("WANDB_MODE", "disabled")

ROOT = Path(__file__).resolve().parent
REPO = ROOT / "Irodori-TTS"


def load_upstream():
    import importlib.util
    sys.path.insert(0, str(REPO))
    spec = importlib.util.spec_from_file_location("irodori_upstream", REPO / "gradio_app_voicedesign.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def info(upstream):
    from irodori_tts.gradio_emoji_palette import EMOJI_PALETTE_ITEMS
    devices = upstream.list_available_runtime_devices()
    return {
        "devices": devices,
        "precisions": {device: upstream._precision_choices_for_device(device) for device in devices},
        "max_candidates": upstream.MAX_GRADIO_CANDIDATES,
        "emojis": [{"emoji": i.emoji, "label": i.label, "description": i.description} for i in EMOJI_PALETTE_ITEMS],
    }


def use_compile(upstream, enabled):
    """Make upstream build compiled runtimes; dynamic shapes avoid a recompile per text length."""
    original = getattr(upstream, "_irodori_build_runtime_key", upstream._build_runtime_key)
    upstream._irodori_build_runtime_key = original
    if enabled:
        upstream._build_runtime_key = lambda **kw: dataclasses.replace(original(**kw), compile_model=True, compile_dynamic=True)
    else:
        upstream._build_runtime_key = original


def generate(upstream, params, out_dir, compile_model=False):
    count = upstream.MAX_GRADIO_CANDIDATES
    use_compile(upstream, compile_model)
    # Upstream saves into "gradio_outputs_voicedesign" under the working directory.
    os.chdir(out_dir)
    result = upstream._run_generation(**params)
    paths = [str(Path(item["value"]).resolve()) for item in result[:count] if item.get("value")]
    detail, timing = result[count:]
    seed = next((line.split(":", 1)[1].strip() for line in detail.splitlines() if line.startswith("seed_used:")), None)
    return {"paths": paths, "log": f"{detail}\n\n{timing}", "seed": seed}


def check(backend):
    """Dependency and device check run by irodori.sh before the server uses this environment."""
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
    protocol = os.fdopen(os.dup(1), "w", encoding="utf-8", buffering=1)
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    upstream = load_upstream()
    for line in sys.stdin:
        try:
            request = json.loads(line)
            op = request["op"]
            if op == "info":
                reply = info(upstream)
            elif op == "generate":
                reply = generate(upstream, request["params"], request["out_dir"], request.get("compile", False))
            elif op == "unload":
                upstream._clear_runtime_cache()
                reply = {}
            else:
                raise ValueError(f"unknown op: {op}")
            reply["ok"] = True
        except Exception as exc:  # reported to the server, which maps it to a user message
            traceback.print_exc()
            reply = {"ok": False, "error_type": type(exc).__name__, "error": str(exc),
                     "trace": traceback.format_exc()}
        protocol.write(json.dumps(reply, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
