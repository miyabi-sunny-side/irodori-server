# HTTP API

JSON の API は `/api` 以下にあります。エラーは `4xx`/`5xx` と `{"error": "<利用者向けの日本語>"}` を返します。
生成の失敗 (`502`) には `{"error": "...", "log": "<詳しい実行記録>"}` が付きます。

## 画面の設定値

`GET /api/info`

```json
{
  "models": [{"id": "Aratako/Irodori-TTS-v4.1-Small", "label": "ベース（標準モデル）"}, ...],
  "modes": [{"id": "design", "label": "言葉で声を作る（ボイスデザイン）"}, ...],
  "devices": ["cpu"],
  "precisions": {"cpu": ["fp32"]},
  "max_candidates": 32,
  "emoji_groups": [{"title": "ポジティブ", "items": [{"emoji": "😊", "label": "楽しげ", "description": "..."}]}]
}
```

`devices`・`precisions`・`emoji_groups` は推論プロセスから取得します。推論プロセスを起動できない場合は `503` です。

## 生成

`POST /api/generate` は生成が終わるまで応答を返しません。生成は 1 件ずつ順に処理します。

```json
{
  "model": "Aratako/Irodori-TTS-v4.1-Small",
  "mode": "design",
  "caption": "落ち着いた女性の声で、やわらかく丁寧に話す。",
  "reference_ids": [3, 1],
  "text": "こんにちは。",
  "dictionary_enabled": true,
  "speed": 1.0,
  "num_steps": 40, "num_candidates": 1, "duration_scale": 1.0,
  "seed": "", "seconds": "",
  "cfg_scale_text": 3.0, "cfg_scale_caption": 4.0, "cfg_scale_speaker": 5.0,
  "model_device": "cpu", "model_precision": "fp32", "codec_device": "cpu", "codec_precision": "fp32",
  "t_schedule_mode": "linear", "sway_coeff": -1.0, "cfg_guidance_mode": "independent",
  "context_kv_cache": true,
  "cfg_scale_raw": "", "speaker_kv_scale_raw": "", "max_text_len_raw": "", "max_caption_len_raw": "",
  "truncation_factor_raw": "", "rescale_k_raw": "", "rescale_sigma_raw": "", "lora_adapter_raw": "",
  "cfg_min_t": 0.5, "cfg_max_t": 1.0
}
```

- 省略した項目は既定値になります。`model_device`・`codec_device` は `/api/info` の `devices` の先頭、
  精度はその機器の `precisions` の先頭です。bf16 を扱える機器 (cuda・xpu) では bf16 が先頭です。
- cuda では torch.compile を使います (LoRA を指定した要求を除く)。最初の生成はコンパイルのため時間がかかります。
- `mode`: `design` (caption を使う)・`clone` (`reference_ids` を使う)・`both`・`auto` (文章だけ)。
  使わない側の caption・参照音声は無視します。`clone`・`both` は参照音声が 1 件以上必要です。
- 範囲: `speed` 0.75〜1.5、`num_steps` 1〜120、`num_candidates` 1〜`max_candidates`、
  `duration_scale` 0.5〜1.5、各 `cfg_scale_*` 0〜10、`sway_coeff` -1〜1.5。
  `*_raw`・`seed`・`seconds` は空欄可の文字列で、推論側が解釈します。

応答 `200`:

```json
{"generations": [<Generation>, ...], "log": "<詳しい実行記録>"}
```

## 動画用の音声

`POST /api/speech` は `/api/generate` と同じ生成要求を受け取り、1 本の WAV をそのまま返します。
候補数は 1 に固定します。生成は履歴に記録され、`/api/generations/{id}` から後で引けます。

```sh
curl -X POST http://<host>:<port>/api/speech -H 'content-type: application/json' \
  -d '{"text": "こんにちは。", "caption": "落ち着いた女性の声で、やわらかく話す。", "seed": "123"}' \
  -o hello.wav
```

応答 `200` は `audio/wav` の本体で、次のヘッダーが付きます。エラーは他の API と同じ JSON です。

- `X-Generation-Id`: 生成の記録の ID
- `X-Seed`: 使ったシード。同じシードと指定で同じ声を作り直せます

1 回の生成で作れる音声は 30 秒までです。長い文章は呼び出し側で文に分けて送ってください。

## 生成の記録

`Generation`:

```json
{
  "id": 12, "batch": "20260926-123456-ab12cd", "candidate": 1, "created_at": "2026-09-26T12:34:56Z",
  "text": "Irodoriです。", "text_applied": "いろどりです。",
  "mode": "design", "caption": "...", "reference_ids": [], "model": "Aratako/Irodori-TTS-v4.1-Small",
  "seed": "1989088249105138954", "speed": 1.0, "params": {<生成要求そのもの>},
  "audio_url": "/api/generations/12/audio"
}
```

- `GET /api/generations` → 記録の一覧 (新しい順) と、消せなかったファイルの一覧

  ```json
  {"generations": [<Generation>, ...], "file_errors": [{"path": "...", "error": "...", "created_at": "..."}]}
  ```

- `GET /api/generations/{id}` → `Generation`
- `GET /api/generations/{id}/audio` → `audio/wav`。`?download=1` で添付ファイルとして返します。
- `DELETE /api/generations/{id}` → `204`。記録を消してから WAV を消します。
  WAV を消せなかった場合は `file_errors` に残ります。

## 参照音声

- `POST /api/references` (multipart、項目名 `file`) で保存し、`{"id": 3, "name": "voice.m4a"}` を返します。
  wav・mp3・flac・ogg・opus・m4a・aac・webm、50MB まで。
- `GET /api/references/{id}/audio` → 保存した音声

## 読み辞書

- `GET /api/dictionary` → `{"entries": [{"word": "Irodori", "reading": "いろどり"}]}`
- `PUT /api/dictionary` `{"word", "reading"}` → `{"message": "読み方を登録しました。", "entries": [...]}`
- `DELETE /api/dictionary/{word}` → `{"message": "選んだ登録を削除しました。", "entries": [...]}`
- `POST /api/dictionary/preview` `{"text", "enabled"}` → `{"text": "<置き換え後>"}`

## モデルの解放

`POST /api/unload` → `{"message": "モデルをメモリから解放しました。次回生成時に再読み込みします。"}`
