"""Small adapter for the pinned upstream UI; all writable paths belong to Easy_test."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent
REPO = ROOT / "Irodori-TTS"
MODELS = [
    ("ベース / Irodori-TTS-v4.1-Small", "Aratako/Irodori-TTS-v4.1-Small"),
    ("Anime / Irodori-TTS-v4.1-Anime", "phasefield-audio/Irodori-TTS-v4.1-Anime"),
]


def load_ui():
    import importlib.util
    source_file = REPO / "gradio_app_voicedesign.py"
    spec = importlib.util.spec_from_file_location("easy_irodori_upstream", source_file)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--backend", choices=["cpu", "cu128", "xpu"], default="cpu")
    parser.add_argument("--port", type=int, default=7860)
    parser.add_argument("--no-browser", action="store_true")
    args = parser.parse_args()
    if not os.environ.get("EASY_IRODORI_ROOT"):
        raise RuntimeError("Please launch Easy_irodori_tts.bat so local storage settings are applied.")
    # Retain the handle: closing it removes this directory from Windows DLL lookup.
    dll_handle = os.add_dll_directory(os.environ["EASY_FFMPEG_BIN"]) if os.name == "nt" else None
    sys.path.insert(0, str(REPO))
    os.chdir(ROOT / "outputs")
    if args.check:
        import torch
        import torchaudio
        import torchcodec
        import soundfile
        if args.backend == "cu128" and not torch.cuda.is_available():
            raise RuntimeError("CUDA is unavailable. Check the NVIDIA driver or restart with -Backend cpu.")
        if args.backend == "xpu" and not torch.xpu.is_available():
            raise RuntimeError("Intel XPU is unavailable. Check the driver or use -Backend cpu.")
        load_ui()
        print(f"Dependency check passed. Python: {sys.executable}; PyTorch: {torch.__version__}")
        return
    ui = load_ui()
    from easy_ui import build_ui, CSS
    demo = build_ui(ui)
    demo.queue(default_concurrency_limit=1)
    demo.launch(
        server_name="127.0.0.1",
        server_port=args.port,
        share=False,
        inbrowser=not args.no_browser,
        css=CSS,
    )


if __name__ == "__main__":
    main()
