# HTTP API

JSON の API は `/api` 以下にあります。エラーは `4xx`/`5xx` と `{"error": "<利用者向けの日本語>"}` を返します。
生成の失敗 (`502`) には `{"error": "...", "log": "<詳しい実行記録>"}` が付きます。

## 画面の設定値

`GET /api/info`

```json
{
  "models": [{"id": "Aratako/Irodori-TTS-v4.1-Small-MF", "label": "MF（高速）", "steps": 4,
              "meanflow": true, "quantized": false, "min_cuda_capability": null}, ...],
  "modes": [{"id": "design", "label": "言葉で声を作る（ボイスデザイン）"}, ...],
  "devices": ["cuda", "cpu"],
  "precisions": {"cuda": ["bf16", "fp32"], "cpu": ["fp32"]},
  "cuda_capability": [8, 6],
  "max_candidates": 32,
  "emoji_groups": [{"title": "ポジティブ", "items": [{"emoji": "😊", "label": "楽しげ", "description": "..."}]}]
}
```

- `models` は標準・Anime・MF・Large (通常・INT8・INT4) で、モデルの性質はサーバーの 1 つの表が持ちます。
  `steps` は計算回数を空欄にしたときの回数、`meanflow` は CFG と Sway を使わないモデル、
  `quantized` は CUDA と bf16 が要るモデル、`min_cuda_capability` は必要な GPU の世代です。
- `devices`・`precisions`・`cuda_capability`・`emoji_groups` は推論エンジンから取得します。推論エンジンに届かない場合は `503` です。

## 生成

`POST /api/generate` は生成が終わるまで応答を返しません。生成は 1 件ずつ順に処理します。

```json
{
  "model": "Aratako/Irodori-TTS-v4.1-Small",
  "mode": "design",
  "caption": "落ち着いた女性の声で、やわらかく丁寧に話す。",
  "reference_ids": [3, 1],
  "voice_id": null,
  "text": "こんにちは。",
  "dictionary_enabled": true,
  "speed": 1.0, "silence": 0.0,
  "num_steps": null, "num_candidates": 1, "duration_scale": 1.0,
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

- 省略した項目は既定値になります。`model_device`・`codec_device` を省略または空欄にすると `/api/info` の `devices` の先頭、
  精度を省略または空欄にするとその機器の `precisions` の先頭です。bf16 を扱える機器 (cuda・xpu) では bf16 が先頭です。
- `num_steps` を省略または `null` にすると、モデルの `steps` (標準 40・MF 4) を使います。
- 量子化版の Large (INT8・INT4) は cuda で bf16 に固定します。cuda や bf16 が無い、INT4 で GPU の世代が足りない場合は `400` です。
- `mode`: `design` (caption を使う)・`clone` (`reference_ids` を使う)・`both`・`auto` (文章だけ)。
  使わない側の caption・参照音声は無視します。`clone`・`both` は参照音声が 1 件以上必要です。
- `voice_id` を指定すると、その話者のモデル・話し方 (caption)・参照音声で生成し、
  `model`・`mode`・`caption`・`reference_ids` は無視します。話し方が空なら `clone`、あれば `both` になります。
  削除した話者や無い話者は `400` です。
- `silence` は末尾に足す無音の秒数 (0〜10) です。
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
  -d '{"voice_id": 3, "text": "こんにちは。", "seed": "123"}' \
  -o hello.wav
```

登録した話者は `GET /api/voices` で一覧できます。話者を指定すると、どの文章も同じ声 (モデル・話し方・参照音声) で生成します。

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
  "seed": "1989088249105138954", "speed": 1.0, "params": {<検証後の生成要求>},
  "audio_url": "/api/generations/12/audio",
  "saved": false,
  "character": "ずんだ",
  "voice_id": 3,
  "favorite": false,
  "scripted": false
}
```

- `GET /api/generations` → 記録の一覧 (新しい順) と、消せなかったファイルの一覧

  ```json
  {"generations": [<Generation>, ...], "file_errors": [{"path": "...", "error": "...", "created_at": "..."}]}
  ```

- `GET /api/generations/{id}` → `Generation`
- `GET /api/generations/{id}/audio` → `audio/wav`。`?download=1` で添付ファイルとして返します。
- `saved` は話者の登録に使った生成 (話者が削除されていないもの) で `true` になります。
- `character` と `voice_id` は、登録した話者で生成したときの話者名と ID です。それ以外は `null` です。
- `scripted` は台本のカードの最新の音声として使われている生成で `true` になります。
- `favorite` は ★ の有無です。`PUT /api/generations/{id}/favorite` `{"favorite": true}` で切り替え、更新後の `Generation` を返します。
- `POST /api/generations/cleanup` は、★・話者の登録・台本での使用のどれにも当たらない生成の記録と WAV を消し、`{"deleted": 5}` を返します。
- `DELETE /api/generations/{id}` → `204`。記録を消してから WAV を消します。
  WAV を消せなかった場合は `file_errors` に残ります。

## 話者 (声)

話者は名前・モデル・話し方 (caption)・参照音声 1 本の組です。登録後に変えられるのは名前だけで、
音声・モデル・話し方を変えるときは別の話者として登録します。

`Voice`:

```json
{"id": 3, "created_at": "2026-10-15T03:00:00Z", "name": "ずんだ",
 "model": "Aratako/Irodori-TTS-v4.1-Small-MF", "caption": "明るく、親しみやすい口調で話す。",
 "source_generation_id": 12, "revision": 1, "audio_url": "/api/voices/3/audio"}
```

- `GET /api/voices` → `{"voices": [<Voice>, ...]}` (登録順。削除した話者は含みません)
- `POST /api/voices` `{"name", "model", "caption", "generation_id" または "reference_id"}` → `Voice`
  - 参照音声は、生成履歴の生成 (`generation_id`) かアップロードした音声 (`reference_id`) のどちらか 1 つです。
    サーバーが WAV に変換して複製するため、元の生成や音声を消しても話者は残ります。
  - 名前は 1 行・1〜80 文字、`model` は `/api/info` の `models` のどれか、話し方は 2000 文字まで、
    参照音声は 1〜120 秒です。外れると `400` です。
- `PATCH /api/voices/{id}` `{"name", "revision"}` → 名前を変えた `Voice` (`revision` が 1 増えます)。
  `revision` が古いと `409`、無い話者は `404` です。
- `DELETE /api/voices/{id}` → `204`。一覧から外しますが参照音声は消さないため、生成済みの音声と処理待ちの生成はそのまま使えます。
- `GET /api/voices/{id}/audio` → 参照音声の WAV

画面の「新しい声を作る」「お手本＋話し方で作る」は、共通のサンプル文章を `POST /api/generate` で候補として生成し、
選んだ候補を `generation_id` で登録します。「手元の音声をそのまま使う」は `POST /api/references` の音声を `reference_id` で登録します。

## 参照音声

- `POST /api/references` (multipart、項目名 `file`) で保存し、`{"id": 3, "name": "voice.m4a", "audio_url": "/api/references/3/audio"}` を返します。
  wav・mp3・flac・ogg・opus・m4a・aac・webm、50MB まで。`id` は生成要求の `reference_ids` と話者の登録に使えます。
- `GET /api/references/{id}/audio` → 保存した音声

## 台本

音声作成の画面の台本は、サーバーに 1 つだけ保存します。別の端末・ブラウザで開いても同じ台本を続きから編集できます。

`ScriptState`:

```json
{
  "revision": 7,
  "script": {
    "cards": [{"id": "9f2c…", "text": "おはよう。", "voice_id": 3, "speed": 1.0, "silence": 0.5}],
    "settings": {<生成要求のうち全カードで共有する項目>},
    "dictionary_enabled": true,
    "options": {"inherit": true, "pattern": "$連番$_$話者名$_$日付$", "txt": false}
  },
  "cards": [{"id": "9f2c…", "status": "done", "error": "", "generation_id": 40,
             "audio_url": "/api/generations/40/audio", "download_name": "001_ずんだ_20261015.wav"}],
  "job": {"pending": [], "running": false, "total": 3, "done": 3, "failed": 0, "stopping": false, "finished": true}
}
```

- `GET /api/script` → `ScriptState`
- `PUT /api/script` `{"revision", "script"}` → `ScriptState`。`revision` が保存済みのものと違えば `409` で、保存しません。
  カードは 1〜1000 枚、ID は重複しない 64 文字まで、文章は 10000 文字まで、`speed` 0.75〜1.5、`silence` 0〜10 です。
  `settings` のうち文章・話者・話速・無音・候補数など、カードごとに決まる項目は保存時に消します。
  `pattern` は 1〜200 文字で、`$連番$`・`$話者名$`・`$テキスト$` (先頭 40 文字)・`$日付$` を置き換えます。
- `cards[].status`: `empty` (文章なし)・`no_voice` (話者が無い)・`new` (未作成)・`queued`・`running`・`done`・`changed` (変更あり)・`failed`。
  変更の判定は、読み辞書を反映した文章・話者・話速・無音・共有の生成設定を、音声を作ったときと比べます。
  読み辞書を変えても反映後の文章が同じなら `done` のままです。
- `POST /api/script/generate` `{"revision", "target"}` で生成を始め、`ScriptState` を返します。
  `target` は `all` (文章のあるカード)・`changed` (`new`・`changed`・`failed`)・カードの ID です。
  話者の無いカードが対象にあると `400`、`revision` が古いか別の生成が進行中なら `409` です。
- 生成はサーバーの中で 1 件ずつ進み、ブラウザを閉じても止まりません。カードの音声は生成履歴にも記録します。
  生成中に文章を変えても、できる音声は開始時の文章のものです。生成に失敗したカードは前の音声を残し、`error` に理由を示します。
- `POST /api/script/stop` → 作成中の 1 件が終わったら止めます。
- `GET /api/script/playlist[?from=<カード ID>]` → `{"items": [{"id", "audio_url"}]}`。文章のあるカードを先頭 (または `from`) から順に返します。
  対応する音声の無いカードがあると `409` で番号を示します。
- `GET /api/script/export?kind=wav` は全カードを台本順につなげた 1 本の WAV、`kind=zip` はカードごとの WAV
  (`options.txt` が真なら文章の TXT も) の ZIP を返します。ZIP の中の名前は `pattern` で付け、同じ名前には `_001` から番号を足します。
  音声の無いカードがあると `409` です。

## 読み辞書

- `GET /api/dictionary` → `{"entries": [{"word": "Irodori", "reading": "いろどり"}]}`
- `PUT /api/dictionary` `{"word", "reading"}` → `{"message": "読み方を登録しました。", "entries": [...]}`
- `DELETE /api/dictionary/{word}` → `{"message": "選んだ登録を削除しました。", "entries": [...]}`
- `POST /api/dictionary/preview` `{"text", "enabled"}` → `{"text": "<置き換え後>"}`

## モデルの解放

`POST /api/unload` → `{"message": "モデルをメモリから解放しました。次回生成時に再読み込みします。"}`

## 推論エンジン

`irodori_engine.py` が `0.0.0.0:${IRODORI_ENGINE_PORT:-7861}` で受ける、irodori-server 向けの内部 API です。
サーバーは `IRODORI_ENGINE_URL` (既定 `http://127.0.0.1:7861`) へ送ります。認証はありません (level-1)。
要求はすべて JSON の `POST` で、エンジンは 1 件ずつ順に処理します。

| path | 要求 | 成功時の応答 (`200`) |
| --- | --- | --- |
| `/info` | `{}` | `{"devices", "precisions", "cuda_capability", "max_candidates", "emojis": [{"emoji", "label", "description"}]}` |
| `/generate` | `{"params": {...}, "references": [{"format": "wav", "data": "<base64>"}]}` | `{"wavs": ["<base64>", ...], "seed": "42", "log": "<実行記録>"}` |
| `/unload` | `{}` | `{}` |

- 応答には常に `ok` (真偽値) と `engine_log` (その要求の処理中にエンジンが出したログ) が付きます。
- 失敗時は `500` と `{"ok": false, "error_type": "ValueError", "error": "...", "trace": "<Python の traceback>"}` です。
  サーバーは `error_type`・`error` を利用者向けの文言に変え、`trace` を生成 API の `502` の `log` に載せます。
- `params` は upstream `_run_generation` のキーワード引数から `ref_wavs` を除いたものです。
  エンジンが `references` を一時ファイルに書き、その順で `ref_wavs` に渡します (空なら `null`)。
  `format` は拡張子で、英小文字と数字の 8 文字までです。
- `wavs` は生成した候補の WAV の本体で、候補の順に並びます。
