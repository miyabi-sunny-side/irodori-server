# irodori-server

[Irodori-TTS](https://github.com/Aratako/Irodori-TTS) の音声合成を、Linux マシンで動かして
ブラウザから試聴・保存するためのサーバーです。

現在の版は、ゆうぷろ氏の Windows 向けアプリ Easy-Irodori-TTS v1.1 を Linux で動くよう移植したものです。
画面の構成・文言・機能は移植元と同じです。

## 必要なもの

- Linux (x86_64)
- `uv`・`git`・`ffmpeg`・`flock` (util-linux)
- FFmpeg 4〜8 の共有ライブラリ。音声の読み書きに使う torchcodec 0.10 は FFmpeg 9 を読めません。
  Arch Linux / Manjaro で `ffmpeg` が 9 の場合は `sudo pacman -S ffmpeg4.4` で追加します。
  共有ライブラリは `/usr/lib` に入り、既存の `ffmpeg` と共存します。
- インターネット接続 (初回に Python・ライブラリ・モデルを取得します) と十分なディスク空き容量
- GPU を使う場合は NVIDIA GPU とドライバー (`nvidia-smi` が動くこと)

## 起動

```sh
./irodori.sh
```

初回は Irodori-TTS の取得と Python 環境の構築に時間がかかります。
`http://127.0.0.1:7860` で待ち受けを始めたら、ブラウザで開きます。ブラウザは自動では開きません。
停止は起動した端末で Ctrl+C です。

別のマシンのブラウザから使う場合は、LAN から届くアドレスで待ち受けます。

```sh
./irodori.sh --host 0.0.0.0 --port 7860
```

| 引数 | 環境変数 | 既定 | 内容 |
| --- | --- | --- | --- |
| `--host` | `IRODORI_HOST` | `127.0.0.1` | 待ち受けるアドレス |
| `--port` | `IRODORI_PORT` | `7860` | 待ち受けるポート |
| `--backend` | `IRODORI_BACKEND` | `auto` | `auto`・`cu128`・`cpu`・`xpu` |

引数は環境変数より優先します。画面に認証はありません。家庭内 LAN など、信頼できる利用者だけが
届く範囲で公開してください。

### GPU と CPU

`auto` では、`nvidia-smi` が GPU を返せば CUDA 12.8 版 (`cu128`)、それ以外は CPU 版を使います。
起動に成功した選択は `config/backend.txt` に保存し、次回の `auto` はそれを使います。
切り替えるときは `--backend` を指定して起動します。CPU は GPU より生成に時間がかかります。

GPU の目安は GTX 16／RTX 20 以降、VRAM 4GB 以上です。AMD 製 GPU には対応していません。

## 使い方

1. 「使うモデル」でベース (標準) か Anime (アニメ調の派生版) を選びます。
   初めて使うモデルは生成時に取得します。
2. 声の作り方を選びます。ボイスデザインは声の説明を文章で、ボイスクローンはお手本の音声を指定します。
   お手本には自分の声か、本人の同意を得た声を使ってください。
3. 読み上げる文章を入力して「音声を生成する」を押します。できた音声は画面で再生し、
   「WAVをダウンロード」で保存できます。

生成設定の「話速（倍）」で 0.75〜1.50 倍を指定できます。声の高さを保って速度を変え、
再生・ダウンロードの両方に反映します。速度を変えたファイルは名前に `_speed_` が付き、
元の音声も残ります。再生音量のスライダーはブラウザに記憶し、WAV の音量は変えません。

### 読み辞書

「読み辞書」タブで表記と読み方を登録します。登録は `config/reading_dictionary.json` に保存し、
次回の起動でも使います。音声作成タブの「読み辞書を使う」で使う／使わないを切り替え、
「読み辞書を反映した文章を確認」で置き換え後の文章を確認できます。

### 絵文字

文章の挿入したい位置をクリックしてから絵文字のボタンを押すと、その位置に入ります。
文字を選択している場合は、その範囲を置き換えます。効果はモデルや文脈で変わります。

## ファイルの置き場所

| 場所 | 内容 |
| --- | --- |
| `Irodori-TTS/` | 固定 revision で取得した Irodori-TTS |
| `.venv/` | Python 3.11 の環境 |
| `outputs/` | 生成した音声 |
| `config/` | `backend.txt`・`reading_dictionary.json`・クレジット表示 |
| `~/.cache/huggingface/` | 音声モデル (Hugging Face の既定の置き場) |

`Irodori-TTS/`・`.venv/`・`outputs/` と `config/` の生成ファイルは Git で管理しません。
削除するときはこのフォルダと、必要なら `~/.cache/huggingface/` のモデルを消します。

## 移植元と改変点

移植元は [ゆうぷろの倉庫](https://uu.getuploader.com/yuupro0308/) の `EasyIrodoriTTS_v1.1.zip` (2026-09-13 公開) です。

- sha256: `c38eb15fd2817818613c58a1881fabf119720602f646ad68906cbfbb435301be`

最初の commit に展開したファイルを原本のまま収め、Linux 化の変更は以降の commit にあります。

- Windows 用の `Easy_irodori_tts.bat`・`.ps1` を `irodori.sh` に置き換えました。
  uv・git・ffmpeg は OS のものを使い、uv・MinGit・FFmpeg のダウンロードはしません。
  Irodori-TTS は移植元と同じ revision `8224dafb46d0aba89209a8f905f1cb7e3299d9c1` に固定します。
- モデルの置き場を配置フォルダ内から `~/.cache/huggingface` に変えました。
- 待ち受けアドレスを指定できるようにし、ブラウザの自動起動をやめました。
- 話速の変更は `PATH` の `ffmpeg` を使います。
- 移植元の説明書 (`★最初に読んでね★.txt`) は、Linux の操作に合わせてこの README にまとめました。

## 開発

```sh
bash tests/launcher_test.sh
```

## ライセンスとクレジット

MIT License です ([LICENSE](LICENSE))。

- Easy-Irodori-TTS 制作：ゆうぷろ (https://www.youtube.com/@yuupro)
- 音声合成エンジン・ベースモデル：[Aratako / Chihiro Arata](https://github.com/Aratako/Irodori-TTS)
- Anime モデル：[phasefield-audio](https://huggingface.co/phasefield-audio/Irodori-TTS-v4.1-Anime)

本人の同意なしに声を複製して公開すること、なりすまし、人をだます目的での使用や誤情報の拡散は禁止です。
禁止事項・免責事項は画面の「クレジット・利用条件」タブと、
[ベースモデル](https://huggingface.co/Aratako/Irodori-TTS-v4.1-Small#license--ethical-restrictions)・
[Anime モデル](https://huggingface.co/phasefield-audio/Irodori-TTS-v4.1-Anime#license)の利用条件を確認してください。
