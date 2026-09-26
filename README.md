# irodori-server

[Irodori-TTS](https://github.com/Aratako/Irodori-TTS) の音声合成を、Linux マシンで動かして
ブラウザから試聴・保存するためのサーバーです。

画面と機能は、ゆうぷろ氏の Windows 向けアプリ Easy-Irodori-TTS v1.1 に揃えています。
Rust のサーバーが推論用の Python プロセスを所有してモデルを常駐させ、生成した音声を
SQLite の記録と WAV ファイルで管理します。過去の生成は履歴の画面から再生・ダウンロード・削除できます。

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
npm --prefix client ci
npm --prefix client run build
cargo build --release
PORT=3000 ./target/release/irodori-server
```

`http://<このマシンのアドレス>:3000` をブラウザで開きます。サーバーは `0.0.0.0` で待ち受けます。
サーバーは作業ディレクトリ (この checkout) の `irodori_worker.py` を `.venv` の Python で推論プロセスとして起動します。
記録と音声は `data/` に保存します。推論プロセスが終了しても、次の生成の前に起動し直します。

| 環境変数 | 既定 | 内容 |
| --- | --- | --- |
| `PORT` | `3000` | 待ち受けるポート |
| `LOG_LEVEL` | `info` | `off`・`error`・`warn`・`info`・`debug`・`trace` |

動画制作などのスクリプトからは `POST /api/speech` で文章を送ると WAV が返ります。
GPU (cuda) では既定で bf16 を使います。fp32 より速く、RTX 3060 では 5.5 秒の音声で 2.1 秒から 1.2 秒に縮みます。
仕組みは [docs/architecture.md](docs/architecture.md)、HTTP API は [docs/api.md](docs/api.md) にあります。

`irodori.sh` は固定 revision の Irodori-TTS を取得し、Python 3.11 の環境 (`uv sync`) を作り、依存と GPU を検査します。
初回は時間がかかります。checkout を更新したときも実行し直します。
画面に認証はありません。家庭内 LAN など、信頼できる利用者だけが届く範囲で公開してください。

### GPU と CPU

`auto` では、`nvidia-smi` が GPU を返せば CUDA 12.8 版 (`cu128`)、それ以外は CPU 版を使います。
検査に通った選択は `config/backend.txt` に保存し、次回の `auto` はそれを使います。
切り替えるときは `./irodori.sh --backend cpu` のように指定し、サーバーを起動し直します。CPU は GPU より生成に時間がかかります。

GPU の目安は GTX 16／RTX 20 以降、VRAM 4GB 以上です。AMD 製 GPU には対応していません。

## 使い方

1. 「使うモデル」でベース (標準) か Anime (アニメ調の派生版) を選びます。
   初めて使うモデルは生成時に取得します。
2. 声の作り方を選びます。ボイスデザインは声の説明を文章で、ボイスクローンはお手本の音声を指定します。
   お手本には自分の声か、本人の同意を得た声を使ってください。
3. 読み上げる文章を入力して「音声を生成する」を押します。できた音声は画面で再生し、
   「WAVをダウンロード」で保存できます。

生成設定の「話速（倍）」で 0.75〜1.50 倍を指定できます。声の高さを保って速度を変え、
再生・ダウンロードの両方に反映します。再生音量のスライダーはブラウザに記憶し、WAV の音量は変えません。
生成した音声は「生成履歴」の画面から、後で再生・ダウンロード・削除できます。

### お手本 (キャラクターの声)

生成履歴で気に入った音声を「お手本に保存」すると、キャラクター名を付けてお手本として残ります。
お手本は音声作成の「お手本の声に似せる」「お手本の声＋話し方を指定」で選び、その声に似せて生成できます。
保存したお手本は「お手本」の画面で再生・削除できます。お手本は WAV の複製なので、元の生成を消しても残ります。

生成履歴の「保存していない音声を一括削除」は、お手本に保存していない生成の記録と WAV をまとめて消します。

### 読み辞書

「読み辞書」の画面で表記と読み方を登録します。登録は `data/irodori.sqlite3` に保存します。音声作成タブの「読み辞書を使う」で使う／使わないを切り替え、
「読み辞書を反映した文章を確認」で置き換え後の文章を確認できます。

### 絵文字

文章の挿入したい位置をクリックしてから絵文字のボタンを押すと、その位置に入ります。
文字を選択している場合は、その範囲を置き換えます。効果はモデルや文脈で変わります。

## ファイルの置き場所

| 場所 | 内容 |
| --- | --- |
| `Irodori-TTS/` | 固定 revision で取得した Irodori-TTS |
| `.venv/` | Python 3.11 の環境 |
| `data/` | irodori-server の記録 (`irodori.sqlite3`)・生成した音声・お手本の音声 |
| `config/` | `backend.txt`・Gradio 版の `reading_dictionary.json` (取り込み元) |
| `~/.cache/huggingface/` | 音声モデル (Hugging Face の既定の置き場) |

`Irodori-TTS/`・`.venv/`・`data/` と `config/` の生成ファイルは Git で管理しません。
irodori-server は初回起動時に、Gradio 版の `config/reading_dictionary.json` を読み辞書へ取り込みます。
元の JSON は変更しません。
削除するときはこのフォルダと、必要なら `~/.cache/huggingface/` のモデルを消します。

## 移植元と改変点

移植元は [ゆうぷろの倉庫](https://uu.getuploader.com/yuupro0308/) の `EasyIrodoriTTS_v1.1.zip` (2026-09-13 公開) です。

- sha256: `c38eb15fd2817818613c58a1881fabf119720602f646ad68906cbfbb435301be`

最初の commit に展開したファイルを原本のまま収め、Linux 化の変更は以降の commit にあります。
その後、画面を Svelte、サーバーを Rust で作り直し、Gradio の画面と起動経路を削除しました。
画面の流れ・用語・生成設定は移植元に揃えています。

- Windows 用の `Easy_irodori_tts.bat`・`.ps1` を `irodori.sh` に置き換えました。
  uv・git・ffmpeg は OS のものを使い、uv・MinGit・FFmpeg のダウンロードはしません。
  Irodori-TTS は移植元と同じ revision `8224dafb46d0aba89209a8f905f1cb7e3299d9c1` に固定します。
- モデルの置き場を配置フォルダ内から `~/.cache/huggingface` に変えました。
- 話速の変更は `PATH` の `ffmpeg` を使います。
- 生成した音声と読み辞書は SQLite と WAV ファイルで管理します。
- 移植元の説明書 (`★最初に読んでね★.txt`) は、Linux の操作に合わせてこの README にまとめました。

## 開発

```sh
bash tests/launcher_test.sh
cargo test
npm --prefix client test
npm --prefix client run test:e2e
```

Rust のテストは推論プロセスの代わりに `tests/support/stub_worker.py` を使うため、`python3` が要ります。
モデルや GPU は要りません。

## ライセンスとクレジット

MIT License です ([LICENSE](LICENSE))。

- Easy-Irodori-TTS 制作：ゆうぷろ (https://www.youtube.com/@yuupro)
- 音声合成エンジン・ベースモデル：[Aratako / Chihiro Arata](https://github.com/Aratako/Irodori-TTS)
- Anime モデル：[phasefield-audio](https://huggingface.co/phasefield-audio/Irodori-TTS-v4.1-Anime)

本人の同意なしに声を複製して公開すること、なりすまし、人をだます目的での使用や誤情報の拡散は禁止です。
禁止事項・免責事項は画面の「クレジット・利用条件」タブと、
[ベースモデル](https://huggingface.co/Aratako/Irodori-TTS-v4.1-Small#license--ethical-restrictions)・
[Anime モデル](https://huggingface.co/phasefield-audio/Irodori-TTS-v4.1-Anime#license)の利用条件を確認してください。
