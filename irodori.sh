#!/usr/bin/env bash
# Linux launcher for Easy-Irodori-TTS (replaces Easy_irodori_tts.bat / .ps1).
# Usage: ./irodori.sh [--backend auto|cu128|cpu|xpu] [--host ADDR] [--port N]
# Env:   IRODORI_BACKEND (auto), IRODORI_HOST (127.0.0.1), IRODORI_PORT (7860)
set -euo pipefail

REVISION=8224dafb46d0aba89209a8f905f1cb7e3299d9c1
UPSTREAM=https://github.com/Aratako/Irodori-TTS.git

# select_backend REQUESTED SAVED GPU_FOUND(0|1)
# An explicit request wins, then the saved choice, then nvidia-smi detection.
select_backend() {
  local backend=$1
  if [[ $backend == auto && -n $2 ]]; then backend=$2; fi
  if [[ $backend == auto ]]; then
    if [[ $3 == 1 ]]; then backend=cu128; else backend=cpu; fi
  fi
  case $backend in cpu | cu128 | xpu) echo "$backend" ;; *) return 1 ;; esac
}

die() {
  echo "ERROR: $*" >&2
  exit 1
}

main() {
  local backend=${IRODORI_BACKEND:-auto} host=${IRODORI_HOST:-127.0.0.1} port=${IRODORI_PORT:-7860}
  while (($#)); do
    case $1 in
      --backend) backend=$2 ;;
      --host) host=$2 ;;
      --port) port=$2 ;;
      *) die "unknown argument: $1 (usage: $0 [--backend auto|cu128|cpu|xpu] [--host ADDR] [--port N])" ;;
    esac
    shift 2
  done
  local root app saved="" gpu=0
  root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  app=$root/Irodori-TTS
  for cmd in uv git ffmpeg flock; do
    command -v "$cmd" >/dev/null || die "$cmd が見つかりません。OS のパッケージで入れてください。"
  done
  mkdir -p "$root/outputs" "$root/config" "$root/temp"
  exec 9>"$root/temp/launcher.lock"
  flock -n 9 || die "このフォルダで別の起動スクリプトが動いています。先に停止してください。"

  # Process-local settings only. HF models stay in the default ~/.cache/huggingface.
  export PYTHONNOUSERSITE=1 PYTHONUTF8=1 PYTHONUNBUFFERED=1 EASY_IRODORI_ROOT=$root \
    UV_PROJECT_ENVIRONMENT=$root/.venv UV_PYTHON_PREFERENCE=only-managed UV_NO_CONFIG=1 \
    GRADIO_ANALYTICS_ENABLED=False HF_HUB_DISABLE_TELEMETRY=1 DO_NOT_TRACK=1 \
    WANDB_MODE=disabled GIT_TERMINAL_PROMPT=0
  unset PYTHONPATH PYTHONHOME VIRTUAL_ENV

  echo "[1/4] Irodori-TTS を固定 revision で用意しています..."
  if [[ ! -d $app/.git ]]; then
    if [[ -e $app && -n $(ls -A "$app") ]]; then
      die "Irodori-TTS フォルダが空でも Git checkout でもありません。退避してから再実行してください。"
    fi
    git init -q "$app"
  fi
  if [[ $(git -C "$app" rev-parse -q --verify HEAD || true) != "$REVISION" ]]; then
    git -C "$app" fetch -q --depth 1 "$UPSTREAM" "$REVISION"
    git -C "$app" checkout -q --detach "$REVISION"
  fi

  [[ -f $root/config/backend.txt ]] && saved=$(tr -d '[:space:]' <"$root/config/backend.txt")
  if command -v nvidia-smi >/dev/null && [[ -n $(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null) ]]; then
    gpu=1
  fi
  backend=$(select_backend "$backend" "$saved" "$gpu") ||
    die "backend は auto・cu128・cpu・xpu のいずれかです (config/backend.txt も確認してください)。"

  echo "[2/4] Python 3.11 の環境を用意しています ($backend)..."
  (cd "$app" && uv sync --frozen --no-dev --extra "$backend" --python 3.11)
  echo "[3/4] 音声ライブラリと GPU を確認しています..."
  "$root/.venv/bin/python" "$root/easy_launcher.py" --check --backend "$backend"
  echo "$backend" >"$root/config/backend.txt"
  echo "[4/4] http://$host:$port で起動します。停止は Ctrl+C です。"
  exec "$root/.venv/bin/python" "$root/easy_launcher.py" --host "$host" --port "$port"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then main "$@"; fi
