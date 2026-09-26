---
version: alpha
name: Sumi / irodori-server
description: Irodori-TTSの音声を作って試聴・保存する管理画面のデザイン契約。
colors:
  primary: "#1d6f5c"
  accent: "#1d6f5c"
  accent-subtle: "rgba(29, 111, 92, 0.10)"
  surface: "#faf6ef"
  surface-raised: "#fffdf8"
  on-surface: "#3a2f28"
  muted: "#6f6257"
  border: "#e3d9c9"
  scrim: "rgba(58, 47, 40, 0.4)"
  link: "#14506e"
  danger: "#9c2b1d"
  danger-subtle: "#f9e9e4"
  wash-base: "#f6efe0"
  wash-raised: "#faf4ea"
  hover-1: "rgba(29, 111, 92, 0.10)"
  hover-2: "rgba(29, 111, 92, 0.16)"
typography:
  title:
    fontFamily: system-ui
    fontSize: 17px
    fontWeight: 600
    lineHeight: 1.3
  body:
    fontFamily: system-ui
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.6
  body-sm:
    fontFamily: system-ui
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: system-ui
    fontSize: 15px
    fontWeight: 500
    lineHeight: 1.2
  caption:
    fontFamily: system-ui
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.4
rounded:
  sm: 6px
  md: 8px
  lg: 12px
  full: 9999px
spacing:
  sp-1: 4px
  sp-2: 8px
  sp-3: 12px
  sp-4: 16px
  sp-5: 24px
components:
  app-header:
    backgroundColor: "{colors.wash-base}"
    textColor: "{colors.on-surface}"
    height: 48px
  sub-header:
    backgroundColor: "{colors.wash-raised}"
    textColor: "{colors.on-surface}"
    height: 40px
  hairline:
    backgroundColor: "{colors.border}"
    height: 1px
  card:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.md}"
    padding: 10px
  panel:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.lg}"
    padding: 16px
  button:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.on-surface}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: 8px
  button-hover:
    backgroundColor: "{colors.hover-1}"
  button-pressed:
    backgroundColor: "{colors.hover-2}"
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.surface-raised}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: 8px
  button-danger:
    backgroundColor: "{colors.danger-subtle}"
    textColor: "{colors.danger}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: 8px
  icon-button:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.sm}"
    size: 36px
  field-help:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.muted}"
    typography: "{typography.caption}"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: 8px
  emoji-button:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-sm}"
    rounded: "{rounded.sm}"
    padding: 4px
  modal:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.lg}"
    padding: 16px
  modal-scrim:
    backgroundColor: "{colors.scrim}"
  radio-selected:
    backgroundColor: "{colors.accent-subtle}"
    rounded: "{rounded.sm}"
  link:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.link}"
  error-banner:
    backgroundColor: "{colors.danger-subtle}"
    textColor: "{colors.danger}"
    typography: "{typography.body-sm}"
    rounded: "{rounded.sm}"
    padding: 8px
  spinner:
    textColor: "{colors.accent}"
    size: 18px
---

# irodori-server

## Overview

Irodori-TTSで音声を作り、ブラウザで試聴して保存する家庭内LAN向けの管理画面。
利用者は別のマシンのブラウザから使い、主操作は「音声を生成する」である。
利用者は先にEasy-Irodori-TTS v1.1 (Gradio版) を使った。画面の流れと用語はそれに合わせ、
「① モデル・声を選ぶ → ② 文章・絵文字を入力 → ③ 音声を生成・保存」を保つ。
入力欄の名前、入力例、説明文もGradio版の文言を使う。

この文書はrust-svelte-templateのDESIGN.md (2026-09-26参照) を取り込んだ製品の契約である。
採用後は本書を正とし、原本の更新は差分を確認して明示的に取り込む。

## Colors

暗色はSumi、通常画面の明色はKinariとする。Sumiから設計し、両方で検証する。
色は `client/src/global.sass` のCSS変数を使う。frontmatterはKinariの値を持つ。

| 役割 | Kinari | Sumi |
|---|---|---|
| surface | #faf6ef | #191919 |
| surface-raised | #fffdf8 | #232323 |
| on-surface | #3a2f28 | #e6e6e6 |
| muted | #6f6257 | #9a9a9a |
| border | #e3d9c9 | #333333 |
| accent / primary | #1d6f5c | #3cc4a4 |
| accent-subtle | rgba(29,111,92,.10) | rgba(60,196,164,.15) |
| link | #14506e | #7fdbff |
| danger | #9c2b1d | #ff6b6b |
| danger-subtle | #f9e9e4 | #3a1a1a |
| scrim | rgba(58,47,40,.4) | rgba(0,0,0,.6) |
| wash-base | #f6efe0 | #232323 |
| wash-raised | #faf4ea | #191919 |
| hover-1 | rgba(29,111,92,.10) | #333333 |
| hover-2 | rgba(29,111,92,.16) | #3d3d3d |

アクセントは「彩り」から採った青緑とする。主ボタンの文字 (surface-raised) との比は
Kinari 5.9:1、Sumi 7.2:1である。surface上の文字ではKinari 5.6:1、Sumi 8.1:1となる。
`primary` はlint用に `accent` と同値を持つ。製品の色名はaccentを使う。

背景・補助情報・通常操作は無彩色を基本とする。帯にはwash、ホバーにはhoverの変数を使う。
アクセントは主操作・フォーカス・小さな選択表示・処理中のスピナー・スライダーのつまみに使う。
塗りつぶす主操作は画面に最大1つとする。音声作成は「音声を生成する」、読み辞書は「登録・更新」。
削除の確定だけはdanger-subtleの面とdangerの文字で示し、主操作の塗りと区別する。
色の意味は文字・形・アクセシビリティ属性でも示す。通常文字は両テーマで4.5:1以上とする。

### テーマの状態

`:root` をSumiの値と `color-scheme: dark` にする。
`data-theme="light"` でKinari、`data-theme="dark"` でSumiを明示指定する。
自動では属性と保存キー `irodori-server:theme` を削除し、OSの `prefers-color-scheme` に従う。
Kinariの明示指定とOS委任は同じSass mixinから出力し、`color-scheme: light` を設定する。
設定はlocalStorageへ保存し、`index.html` の小さなscriptで初回描画前に適用する。

## Typography

書体はsystem-uiとし、Webフォントを追加しない。サイズ・太さ・行高はfrontmatterの5役割を使う。
パネルの見出し (①②③) とページ見出しはtitle、入力欄の名前はlabel、
入力欄の下の説明はcaptionのmutedとする。文章入力と音声の文字は本文16pxを保つ。

## Layout

### 画面とURL

| URL | 画面 | 内容 |
|---|---|---|
| `/` | 音声作成 | ①②③の3パネル、生成設定、詳細設定、実行記録 |
| `/dictionary` | 読み辞書 | 登録・更新の入力、登録済みの一覧 |
| `/history` | 生成履歴 | 保存された生成の一覧 (新しい順) |
| `/credits` | クレジット・利用条件 | 制作者、エンジン、モデル、ライセンス、禁止事項 |
| `/references` | お手本 | 生成履歴から保存したお手本 (キャラクター名ごと) |

- app headerは全幅・高さ48px・stickyで、wash-baseと1pxの下境界を使う。
  左に音声作成へ戻るアプリ名、右に36pxのメニューボタンだけを置く。
- sub-headerは高さ40px・header下にsticky、wash-raisedと1pxの下境界で、
  現在の画面名だけを1行で示す。ボタンやリンクを置かない。
- 本文は通常の文書フローで続き、mainに独立した縦スクロール領域を作らない。
  左右の余白は12px、上下は狭幅16px・広幅24px。幅320px以上で横にはみ出さない。

### 音声作成

入力の状態は画面を移っても保つ (モジュールの状態に置く)。生成中に読み辞書へ移っても
生成は続き、戻ると結果が表示される。

- 幅1100px以上では3パネルを横に並べ、列の比は3:5:3、間隔16px、最大幅1440pxとする。
  1440×900で①②③の主要項目と「音声を生成する」がスクロールなしで見える。
- 1100px未満は①②③を縦に並べ、最大幅720pxの中央1列とする。
- パネルはsurface-raised、1px枠、12px角丸、16px余白で、見出し「① モデル・声を選ぶ」
  「② 文章・絵文字を入力」「③ 音声を生成・保存」を持つ。
- ①: 使うモデル、声の作り方、(クローン系) お手本の音声、(デザイン系) 声・話し方の説明と
  説明の入力例、読み辞書を使うチェックと読み辞書へのリンク。
  声の作り方に不要な入力欄は表示しない。
- ②: 読み上げる文章 (6行)、絵文字の案内1行、絵文字の分類タブ、
  「読み辞書を反映した文章を確認」。確認結果は押した後にだけ表示する。
- ③: 「音声を生成する」、現在の状態、(候補2件以上) 聞き比べる音声、音声プレーヤー、
  再生音量、「WAVをダウンロード」、生成履歴へのリンク。
  音声が無い間はプレーヤーとダウンロードを表示しない。
- パネルの下に「生成設定」「詳細設定（通常は変更不要）」「実行記録（問い合わせ用）」を
  閉じた `details` で置く。開くと入力欄が2列 (768px未満は1列) で並ぶ。

### 読み辞書・生成履歴・お手本・クレジット

最大幅720pxの中央1列とする。

- 読み辞書: 表記と読み方の入力 (768px以上は横並び)、入力例、「登録・更新」、結果の文、
  登録済みの表 (表記・読み方・編集・削除)、置き換えのルールの説明の順。
- 生成履歴: カードの1列。各カードに文章 (2行で省略)、日時・声の作り方・モデル・話速、
  「お手本に保存」(小さな通常ボタン、book。★ はお気に入りのために残す) または「お手本に保存済み」(accentの文字とcheck-check)、
  プレーヤー、ダウンロード、削除を置く。削除できなかったファイルがあるときだけ、
  一覧の前にerror-bannerで示す。
  一覧の前に「保存していない音声を一括削除（N件）」(通常ボタン、trash) と、
  お手本に保存した音声は残る旨・お手本の一覧へのリンクを置く。対象0件では押せない。
  結果 (「N件の音声を削除しました。」「「名前」のお手本に保存しました。」) は `role=status` の1行で示す。
- お手本に保存: モーダルで元の文章 (2行で省略)、キャラクター名 (必須・1行・50文字、
  既存の名前を `datalist` で候補に出す)、キャンセルと「保存する」(このモーダルの主操作) を置く。
  名前の誤りは入力欄の下にdangerの文で示し、`aria-invalid` を付ける。保存後は
  「お手本に保存済み」へフォーカスを移す。
- お手本: キャラクター名の見出し (title、件数をcaptionで併記) ごとにカードの1列。
  各カードに名前・元の生成ID・保存日時、プレーヤー、削除を置く。
  空のときは生成履歴の「お手本に保存」で追加できる旨と生成履歴へのリンクを示す。
- クレジット: Gradio版のクレジットタブと同じ内容を見出しと箇条書きで示す。

## Elevation & Depth

階層は面の濃淡と1pxの境界で示す。
影はメニューとモーダルの `0 8px 32px rgba(0,0,0,.25)` だけに使う。
フォーカスは共通の `:focus-visible` に2pxのaccent色の輪郭と2pxの間隔を設ける。

## Shapes

角丸は小部品6px、カード8px、パネル・モーダル・メニュー12pxとする。
円形ボタンを作らない。同じ操作部品で角丸を混ぜない。

## Components

### アイコン

`client/src/lib/Icon.svelte` を辞書の正とし、列挙は `ICON_NAMES` を使う。
絵文字や文字記号をアイコンの代わりに使わない。絵文字パレットの絵文字は入力する内容である。
SVGは24×24、currentColorの2px線、丸い端と角、1.2emとする。
この製品では `chevron-up`・`chevron-down`・`download`・`upload` を追加した。

### メニューとテーマ設定

メニューは右上のボタンに接するドロップダウンで、上端をheader下端、右端をボタン右端へそろえる。
最小幅180px、1px枠、12px角丸、surface-raised。項目は上下8px・左右12pxの全幅の行とする。
項目は「テーマ設定」「読み辞書」「生成履歴」「お手本」「クレジット・利用条件」の順。
音声作成はアプリ名から戻るため項目に置かない。
外側クリック・Escで閉じ、フォーカスをメニューボタンへ戻す。開閉は `aria-expanded` に反映する。

テーマ設定は中央のモーダルで、自動・ライト・ダークの3つのラジオにmonitor/sun/moonを使う。
選択は即時反映し、モーダルは閉じない。閉じるボタン・Esc・scrimで閉じる。

### 入力欄

- ラベルは上に置き、説明はその下のcaptionとし、`aria-describedby` で入力欄へ結ぶ。
- 入力欄とselectはsurface、1px枠、6px角丸、本文サイズ。フォーカスはaccentの枠と共通リング。
- スライダーはrangeと数値入力の組とし、どちらを変えても同じ値になる。
  数値入力は範囲外を範囲内へ丸める。アクセントはrangeのつまみ (`accent-color`) だけに使う。
- 無効な入力欄は不透明度50%とし、理由を説明文で示す (Swayの調整値など)。

### ボタン

- 通常ボタンはsurface-raised、1px枠、6px角丸、上下8px・左右14px。ホバーはhover-1。
- 主ボタンはaccentで塗り、文字はsurface-raised。「音声を生成する」はパネル幅いっぱい・高さ44px。
- 無効時は不透明度50%でポインターを付けない。生成中は「音声を生成する」と
  モデル解放を無効にし、状態欄にスピナーと「生成しています…」を示す。
- ダウンロードはボタンの見た目のリンク (`a[download]`) とし、download アイコンを付ける。

### 絵文字パレット

- 分類をタブ (`role=tablist`) で切り替え、選んだタブを下線2pxのaccentと太字で示す。
- 各絵文字は「絵文字 ラベル」のボタンで、surface、1px枠、6px角丸、4px 8pxの余白、
  6px間隔で折り返す。説明は `title` に入れる。aria-labelは「ラベルの絵文字を挿入」。
- 押しても文章欄のカーソル位置を保ち、そこへ挿入する。選択中の文字は置き換える。
  挿入後は文章欄にフォーカスを戻し、カーソルを挿入した絵文字の後ろに置く。

### お手本の音声

「音声ファイルを追加」(upload) でファイルを選ぶと、その場でアップロードする。
追加した音声は名前を1列に並べる。各行には「上に移動」「下に移動」「削除」のアイコンボタンを置く。
アップロード中はスピナー、失敗は一覧の下にdangerの文で理由を示し、再度選び直せる。
保存したお手本があるときは、アップロードの前に「保存したお手本から追加…」のselect
(キャラクター名の `optgroup`) を置く。選ぶと一覧の末尾に加え (同じお手本は重ねない)、selectは未選択に戻す。
一覧の名前は保存したお手本なら「キャラクター名 / 生成 N」と示す。

### 状態と結果

- 状態欄は `role=status` で、通常はmuted、失敗はdangerの文字で示す。
  失敗の原因は利用者向けの文、詳しい実行記録は「実行記録」を開いて確認する。
- 再生音量はrange (0〜100%、既定75%) で、`irodori-server:playback-volume` に0〜1で保存する。
  音声要素の音量だけを変え、WAVは変えない。
- 候補が2件以上のときだけ「聞き比べる音声」のselect (音声 1, 音声 2 …) を示す。

### 一覧とモーダル

- 生成履歴のカードはsurface-raised、1px枠、8px角丸、10px余白、8px間隔。
- 一覧の `data-state` はloading/empty/error/successとする。
  読込中は14pxの文字とスピナー、失敗はdangerの説明と「再読み込み」を示す。
- 削除は確認のモーダル (幅360px) で、キャンセルと「削除する」(danger) を置く。
  閉じるボタン・Esc・scrimで閉じ、フォーカスを押したボタンへ戻す。
- 動きは150ms以下の変化とスピナーに限り、`prefers-reduced-motion: reduce` では止める。

## Verification

実装はSassの字下げ構文を使い、normalize.cssを先に読み込む。
変数の接頭辞は色が `--c-*`、余白が `--sp-1..5`、文字が `--fs-xs..xl`、角丸が `--radius-*` とする。
`designmd lint` は形式を検査する。UIへの適用は実ブラウザで次を確認する。

- `npm --prefix client run test:e2e` はbackendを `page.route` で置き換え、Chromiumで次を測る。
  音声作成・読み辞書・生成履歴の代表操作、1440×900と390×844の明暗で横はみ出し0px、
  1440×900で「音声を生成する」が最初の画面に入ること。
- Sumiの背景色はrgb(25,25,25)、Kinariの背景色はrgb(250,246,239)とする。明示指定がOSより優先される。
- header内の操作対象はアプリ名とメニューボタンの2つだけとする。
- 変更時は同じデータと画面サイズで前後を比べ、主要情報の面積と色の強さを確認する。
