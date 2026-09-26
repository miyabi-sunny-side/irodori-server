"""Stand-in for irodori_worker.py in tests: same protocol, no model, tiny WAV files."""
import json
import os
import sys
import wave

for line in sys.stdin:
    request = json.loads(line)
    op = request["op"]
    if op == "die":
        os._exit(3)
    if op == "info":
        reply = {"devices": ["cpu"], "precisions": {"cpu": ["fp32"]}, "max_candidates": 4,
                 "emojis": [{"emoji": "😊", "label": "楽しげ", "description": "楽しそうに"}]}
    elif op == "fail":
        reply = {"ok": False, "error_type": "ValueError", "error": "bad", "trace": "Traceback: bad"}
    elif op == "generate":
        params = request["params"]
        if params["text"] == "落ちる":
            os._exit(3)
        out = os.path.join(request["out_dir"], "gradio_outputs_voicedesign")
        os.makedirs(out, exist_ok=True)
        paths = []
        for i in range(int(params["num_candidates"])):
            path = os.path.join(out, f"sample_{i + 1:03d}.wav")
            with wave.open(path, "wb") as audio:
                audio.setnchannels(1)
                audio.setsampwidth(2)
                audio.setframerate(8000)
                audio.writeframes(b"\0\0" * 800)
            paths.append(path)
        reply = {"paths": paths, "seed": "42", "log": "stub\n" + json.dumps(params, ensure_ascii=False)}
    elif op == "unload":
        reply = {}
    if "ok" not in reply:
        reply["ok"] = True
    print(json.dumps(reply, ensure_ascii=False), flush=True)
