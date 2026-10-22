# irodori-server

[Irodori-TTS](https://github.com/Aratako/Irodori-TTS) の音声合成を、Linux マシンで動かして
ブラウザから試聴・保存するためのサーバーです。

画面と機能は、ゆうぷろ氏の Windows 向けアプリ Easy-Irodori-TTS v2.0 に揃えています。
声 (モデル・話し方・お手本の音声の組) を話者として登録しておくと、どのセリフも同じ声で生成できます。
複数の話者のセリフを台本としてまとめて生成し、続けて聞いたり、1 本の WAV や ZIP で保存したりできます。
推論は Python の推論エンジン (`irodori_engine.py`) が HTTP で受け持ち、モデルを常駐させます。
Rust のサーバーは、生成した音声を SQLite の記録と WAV ファイルで管理します。
推論エンジンは別のマシンに置けるため、GPU のないマシンのサーバーから GPU マシンのエンジンを使えます。過去の生成は履歴の画面から再生・ダウンロード・削除できます。

## 必要なもの

- Linux (x86_64)
- `uv`・`git`・`ffmpeg`・`flock` (util-linux)
- FFmpeg 4〜8 の共有ライブラリ。音声の読み書きに使う torchcodec 0.10 は FFmpeg 9 を読めません。
  Arch Linux / Manjaro で `ffmpeg` が 9 の場合は `sudo pacman -S ffmpeg4.4` で追加します。
  共有ライブラリは `/usr/lib` に入り、既存の `ffmpeg` と共存します。
- インターネット接続 (初回に Python・ライブラリ・モデルを取得します) と十分なディスク空き容量
- GPU を使う場合は NVIDIA GPU とドライバー (`nvidia-smi` が動くこと)

## irodori-server の起動

ビルドには Rust 1.96 以降と Node.js 24 が要ります。

```sh
./irodori.sh                      # Irodori-TTS と Python 環境を用意する
.venv/bin/python irodori_engine.py  # 推論エンジン (別の端末か systemd で動かし続ける)

npm --prefix client ci
npm --prefix client run build
cargo build --release
PORT=3000 ./target/release/irodori-server
```

`http://<このマシンのアドレス>:3000` をブラウザで開きます。サーバーは `0.0.0.0` で待ち受けます。
記録と音声はサーバーの作業ディレクトリの `data/` に保存します。

推論エンジンは `0.0.0.0:7861` で待ち受け、要求を 1 件ずつ順に処理します。
サーバーは `IRODORI_ENGINE_URL` のエンジンを呼び、起動・再起動はしません。
エンジンを止めている間の生成は `503` になり、エンジンを起動し直せば次の生成から使えます。
別のマシンのエンジンを使うときは、サーバー側に `irodori.sh` の環境は要りません。

```sh
IRODORI_ENGINE_URL=http://192.168.1.100:7861 PORT=3000 ./target/release/irodori-server
```

| 環境変数 | 対象 | 既定 | 内容 |
| --- | --- | --- | --- |
| `PORT` | サーバー | `3000` | 待ち受けるポート |
| `LOG_LEVEL` | サーバー | `info` | `off`・`error`・`warn`・`info`・`debug`・`trace` |
| `IRODORI_ENGINE_URL` | サーバー | `http://127.0.0.1:7861` | 推論エンジンの URL |
| `IRODORI_ENGINE_PORT` | エンジン | `7861` | エンジンが待ち受けるポート |

推論中にエンジンが出したログ (upstream のログや例外) は応答に載り、呼び出したサーバーのログに書かれます。
エンジン自体が落ちた原因は、エンジンを動かすマシンの標準エラー (systemd なら journal) に残ります。

動画制作などのスクリプトからは `POST /api/speech` で文章と登録した話者 (`voice_id`) を送ると WAV が返ります。
GPU (cuda) では既定で bf16 を使います。fp32 より速く、RTX 3060 では 5.5 秒の音声で 2.1 秒から 1.2 秒に縮みます。
仕組みは [docs/architecture.md](docs/architecture.md)、HTTP API は [docs/api.md](docs/api.md) にあります。

`irodori.sh` は固定 revision の Irodori-TTS を取得し、Python 3.11 の環境 (`uv sync`) を作り、依存と GPU を検査します。
初回は時間がかかります。checkout を更新したときも実行し直します。
画面と推論エンジンに認証はありません。家庭内 LAN など、信頼できる利用者だけが届く範囲で公開してください。

### GPU と CPU

`auto` では、`nvidia-smi` が GPU を返せば CUDA 12.8 版 (`cu128`)、それ以外は CPU 版を使います。
検査に通った選択は `config/backend.txt` に保存し、次回の `auto` はそれを使います。
切り替えるときは `./irodori.sh --backend cpu` のように指定し、推論エンジンを起動し直します。CPU は GPU より生成に時間がかかります。

GPU の目安は GTX 16／RTX 20 以降、VRAM 4GB 以上です。AMD 製 GPU には対応していません。

## 使い方

### 声を登録する

メニューの「声の登録」で、作り方を選んで話者を登録します。話者名・モデル・登録後の話し方・お手本の音声が 1 組になり、
登録後に変えられるのは名前だけです。音声やモデルを変えたいときは別の話者として登録します。

- 新しい声を作る: 声の説明を書く (空欄ならおまかせ) と、共通のサンプル文章で候補を作ります。候補を聞いて 1 つ選びます。
- 手元の音声をそのまま使う: 1〜120 秒の音声ファイルをそのまま使います。10 秒未満は声の再現が不安定になる場合があります。
  任意で短い文章をテストできます。
- お手本＋話し方で作る: お手本の音声に話し方の指定を加えて候補を作り、選びます。

お手本には自分の声か、本人の同意を得た声を使ってください。
生成履歴の音声も「話者に登録」から登録でき、その生成のモデルと話し方を引き継ぎます。

モデルは標準・Anime・MF（高速）・Large（通常版・INT8・INT4）から選び、初めて使うときに取得します。
MF は計算回数 4 回で速いモデルです。Large の INT8・INT4 は NVIDIA GPU 専用で、INT4 は RTX 30 シリーズ以降が目安です。

### 台本を作る

「音声作成」の画面で、セリフのカードごとに話者・文章・話速 (0.75〜1.50 倍)・末尾の無音 (0〜10 秒) を指定します。
カードは 1000 枚まで追加・複製・削除でき、左端のつまみをドラッグするか、つまみで Alt+↑／↓ を押すと並べ替えられます。
長いセリフは推定秒数が赤くなり、はさみのボタンで文の切れ目から分割できます。
カードの絵文字のボタンから、文章のカーソル位置へ感情や効果の絵文字を入れられます。
入力は自動でサーバーに保存され、別の端末やブラウザで開いても続きから編集できます。

「未生成・変更分を生成」で、まだ音声が無いカードと、音声を作った後に文章・話者・話速・無音・生成設定を変えたカードを生成します。
生成はサーバーで 1 件ずつ進むため、ブラウザを閉じても止まりません。「停止」は作成中の 1 件が終わってから止まります。
できた音声は生成履歴にも残ります。

「連続再生」「選択から再生」で台本の順に聞き、「つなげて保存」で 1 本の WAV、「個別にZIP」でカードごとの WAV をまとめて保存します。
ファイル名の付け方と、ZIP に文章 (TXT) も入れるかは「オプション」で選びます。
「入力済データ」では文章・話者の指定・設定をファイルに保存して呼び出せます (Easy-Irodori-TTS v2.0 のファイルも読めます)。

### 生成履歴と ★

生成した音声は「生成履歴」の画面で再生・ダウンロード・削除でき、話者名で絞り込めます。気に入った音声には ★ を付けます。
一括削除は、★ を付けた音声、話者の登録に使った音声、台本で使っている音声を残し、ほかの生成の記録と WAV をまとめて消します。

### 読み辞書

「読み辞書」の画面で表記と読み方を登録し、音声作成の画面の「読み辞書を使う」で使う／使わないを切り替えます。

## ファイルの置き場所

| 場所 | 内容 |
| --- | --- |
| `Irodori-TTS/` | 固定 revision で取得した Irodori-TTS |
| `.venv/` | Python 3.11 の環境 |
| `data/` | irodori-server の記録 (`irodori.sqlite3`)・生成した音声・話者の音声・アップロードした音声 |
| `config/` | `backend.txt`・Gradio 版の `reading_dictionary.json` (取り込み元) |
| `~/.cache/huggingface/` | 音声モデル (Hugging Face の既定の置き場) |

`Irodori-TTS/`・`.venv/`・`data/` と `config/` の生成ファイルは Git で管理しません。
irodori-server は初回起動時に、Gradio 版の `config/reading_dictionary.json` を読み辞書へ取り込みます。
元の JSON は変更しません。
削除するときはこのフォルダと、必要なら `~/.cache/huggingface/` のモデルを消します。

## 移植元と改変点

移植元は [ゆうぷろの倉庫](https://uu.getuploader.com/yuupro0308/) の Easy-Irodori-TTS です。

| 版 | ファイル | sha256 |
| --- | --- | --- |
| v1.1 (2026-09-13 公開) | `EasyIrodoriTTS_v1.1.zip` | `c38eb15fd2817818613c58a1881fabf119720602f646ad68906cbfbb435301be` |
| v2.0 (2026-10-10 公開) | `EasyIrodoriTTS_v2.0.zip` | `229129871ccefcd9eb669ad8aba85f65a3c2a29d31dd2cae20329560e476ad87` |
| v2.0 Large 版 | `EasyIrodoriTTS_large_v2.0.zip` | `8f33b44a371044582e31715756ea16c2c2bd9cb9d98f79651e6e8f1d1f7e85e2` |

最初の commit に v1.1 を原本のまま収め、Linux 化の変更は以降の commit にあります。
その後、画面を Svelte、サーバーを Rust で作り直し、Gradio の画面と起動経路を削除しました。
v2.0 の話者の登録と台本は、画面の流れ・用語・入力例・生成設定を移植元に揃えて作り直しました。
v2.0 の通常版と Large 版は別の配布物ですが、話者ごとにモデルを選ぶため、irodori-server は 1 つで両方のモデルを扱います。

- Windows 用の `Easy_irodori_tts.bat`・`.ps1` を `irodori.sh` に置き換えました。
  uv・git・ffmpeg は OS のものを使い、uv・MinGit・FFmpeg のダウンロードはしません。
  Irodori-TTS は移植元 v2.0 と同じ revision `89f9d8fbd4d51ea019867ee1197725ede1df13c5` に固定します。
- モデルの置き場を配置フォルダ内から `~/.cache/huggingface` に変えました。
- 話速と末尾の無音の加工は `PATH` の `ffmpeg` を使います。
- 生成した音声・話者・台本・読み辞書は SQLite と WAV ファイルでサーバーが管理します。
  台本はブラウザではなくサーバーに保存し、元に戻す・やり直すは画面ごとに 100 回までです。
- 保存はブラウザのダウンロードで渡します。保存先フォルダの選択と自動保存、フォルダを開く操作は取り込んでいません。
  ブラウザの自動起動、ごみ箱への移動、localhost 限定も取り込まず、CPU/GPU は `irodori.sh --backend` で切り替えます。
- 移植元の説明書 (`★最初に読んでね★.txt`) は、Linux の操作に合わせてこの README にまとめました。

## 開発

commit の前に、手元で次をすべて通します。GitHub Actions では検査・テストを走らせません。

```sh
npm --prefix client ci
npm --prefix client run format:check
npm --prefix client run check
npm --prefix client test
npm --prefix client run test:e2e  # 初回は npx --prefix client playwright install chromium
npm --prefix client run build
npm --prefix client run lint:design
shellcheck -x irodori.sh tests/launcher_test.sh
bash tests/launcher_test.sh
python3 -m unittest tests/engine_test.py
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Rust のテストは推論エンジンの代わりに同じ HTTP 契約の `tests/support/stub_engine.py` を使うため、`python3` が要ります。
話者の音声の変換と台本の書き出しを確かめるテストには `ffmpeg` が要ります。モデルや GPU は要りません。

## ライセンスとクレジット

MIT License です ([LICENSE](LICENSE))。

- Easy-Irodori-TTS 制作：ゆうぷろ (https://www.youtube.com/@yuupro) with Codex
- 音声合成エンジン・ベースモデル：[Aratako / Chihiro Arata](https://github.com/Aratako/Irodori-TTS)
- Anime モデル：[phasefield-audio](https://huggingface.co/phasefield-audio/Irodori-TTS-v4.1-Anime)
- MF モデル：[Aratako / Irodori-TTS-v4.1-Small-MF](https://huggingface.co/Aratako/Irodori-TTS-v4.1-Small-MF)
- Large モデル：[Irodori-TTS-v4-Large](https://huggingface.co/Aratako/Irodori-TTS-v4-Large)・[Irodori-TTS-v4-Large-Quantized](https://huggingface.co/Aratako/Irodori-TTS-v4-Large-Quantized)。
  MIT License ではなく、[Gemma 利用規約](https://ai.google.dev/gemma/terms)と[禁止用途ポリシー](https://ai.google.dev/gemma/prohibited_use_policy)が適用されます。

本人の同意なしに声を複製して公開すること、なりすまし、人をだます目的での使用や誤情報の拡散は禁止です。
禁止事項・免責事項は画面の「クレジット・利用条件」タブと、
[ベースモデル](https://huggingface.co/Aratako/Irodori-TTS-v4.1-Small#license--ethical-restrictions)・
[Anime モデル](https://huggingface.co/phasefield-audio/Irodori-TTS-v4.1-Anime#license)・
[MF モデル](https://huggingface.co/Aratako/Irodori-TTS-v4.1-Small-MF)の利用条件を確認してください。
