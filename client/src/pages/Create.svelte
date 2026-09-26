<script lang="ts">
  import { tick } from "svelte";

  import { api, ApiError, downloadUrl } from "../lib/api";
  import { insertText, moveItem } from "../lib/create";
  import Icon from "../lib/Icon.svelte";
  import Slider from "../lib/Slider.svelte";
  import {
    create,
    generate,
    info,
    loadInfo,
    playback,
    saveVolume,
    unload,
  } from "../lib/state.svelte";

  const CAPTION_EXAMPLES = [
    "落ち着いた女性の声で、やわらかく丁寧に話す。",
    "明るく元気な声で、楽しそうに話す女性。",
    "低めの男性の声で、ゆっくり穏やかに話す。",
  ];
  const RAW_FIELDS = [
    ["cfg_scale_raw", "反映度の一括上書き"],
    ["speaker_kv_scale_raw", "お手本の内部補正値"],
    ["max_text_len_raw", "文章の処理上限（トークン数）"],
    ["max_caption_len_raw", "説明の処理上限（トークン数）"],
    ["truncation_factor_raw", "ノイズの打ち切り係数"],
    ["rescale_k_raw", "再スケーリング係数"],
    ["rescale_sigma_raw", "再スケーリングの強さ"],
    ["lora_adapter_raw", "LoRAフォルダの場所"],
  ] as const;

  const form = create.form;
  let textArea = $state<HTMLTextAreaElement | undefined>();
  let emojiTab = $state(0);
  let preview = $state<string | null>(null);
  let previewError = $state("");
  let uploading = $state(false);
  let uploadError = $state("");

  let usesReferences = $derived(form.mode === "clone" || form.mode === "both");
  let usesCaption = $derived(form.mode === "design" || form.mode === "both");
  let current = $derived(create.generations[create.selected]);
  let groups = $derived(info.value?.emoji_groups ?? []);
  let precisionsFor = (device: string) =>
    info.value?.precisions[device] ?? [form.model_precision];

  $effect(() => {
    void loadInfo();
  });

  async function insertEmoji(emoji: string) {
    const area = textArea;
    const result = insertText(
      form.text,
      area?.selectionStart ?? null,
      area?.selectionEnd ?? null,
      emoji,
    );
    form.text = result.text;
    await tick();
    area?.focus({ preventScroll: true });
    area?.setSelectionRange(result.caret, result.caret);
  }

  function onTabKey(event: KeyboardEvent) {
    const delta =
      event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (delta === 0 || groups.length === 0) {
      return;
    }
    event.preventDefault();
    emojiTab = (emojiTab + delta + groups.length) % groups.length;
    document.getElementById(`emoji-tab-${emojiTab}`)?.focus();
  }

  async function showPreview() {
    previewError = "";
    try {
      preview = (await api.preview(form.text, form.dictionary_enabled)).text;
    } catch (error) {
      previewError =
        error instanceof ApiError ? error.message : "確認できませんでした。";
    }
  }

  async function addReferences(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const files = [...(input.files ?? [])];
    input.value = "";
    uploadError = "";
    uploading = true;
    try {
      for (const file of files) {
        create.references.push(await api.uploadReference(file));
      }
    } catch (error) {
      uploadError =
        error instanceof ApiError
          ? error.message
          : "お手本の音声を追加できませんでした。";
    } finally {
      uploading = false;
    }
  }

  function changeDevice(which: "model" | "codec") {
    const device = which === "model" ? form.model_device : form.codec_device;
    const first = precisionsFor(device)[0];
    if (which === "model") {
      form.model_precision = first;
    } else {
      form.codec_precision = first;
    }
  }
</script>

<div class="content create">
  {#if info.error}
    <div class="error-banner" role="alert">
      <span>推論プロセスの設定を読み込めませんでした。{info.error}</span>
      <button class="btn" type="button" onclick={() => void loadInfo()}>
        再試行
      </button>
    </div>
  {/if}

  <div class="panels">
    <section class="panel" aria-labelledby="step-1">
      <h2 id="step-1">① モデル・声を選ぶ</h2>
      <div class="field">
        <label for="model">使うモデル</label>
        <select
          id="model"
          class="input"
          bind:value={form.model}
          aria-describedby="model-help"
        >
          {#each info.value?.models ?? [] as model (model.id)}
            <option value={model.id}>{model.label}</option>
          {/each}
        </select>
        <p id="model-help" class="help">
          ベースは標準、Animeはアニメ調の派生モデルです。
        </p>
      </div>
      <div class="field">
        <label for="mode">声の作り方</label>
        <select id="mode" class="input" bind:value={form.mode}>
          {#each info.value?.modes ?? [] as mode (mode.id)}
            <option value={mode.id}>{mode.label}</option>
          {/each}
        </select>
      </div>

      {#if usesReferences}
        <div class="field references">
          <span class="label" id="references-label"
            >お手本の音声（同意を得た声）</span
          >
          {#if create.references.length > 0}
            <ol class="reference-list" aria-labelledby="references-label">
              {#each create.references as reference, index (reference.id)}
                <li>
                  <span class="reference-name">{reference.name}</span>
                  <button
                    class="icon-btn"
                    type="button"
                    aria-label={`${reference.name}を上に移動`}
                    disabled={index === 0}
                    onclick={() =>
                      (create.references = moveItem(
                        create.references,
                        index,
                        -1,
                      ))}><Icon name="chevron-up" /></button
                  >
                  <button
                    class="icon-btn"
                    type="button"
                    aria-label={`${reference.name}を下に移動`}
                    disabled={index === create.references.length - 1}
                    onclick={() =>
                      (create.references = moveItem(
                        create.references,
                        index,
                        1,
                      ))}><Icon name="chevron-down" /></button
                  >
                  <button
                    class="icon-btn"
                    type="button"
                    aria-label={`${reference.name}を削除`}
                    onclick={() =>
                      (create.references = create.references.filter(
                        (item) => item.id !== reference.id,
                      ))}><Icon name="x" /></button
                  >
                </li>
              {/each}
            </ol>
          {/if}
          <label class="btn upload" class:busy={uploading}>
            {#if uploading}
              <span class="spinner" aria-hidden="true"></span>追加しています…
            {:else}
              <Icon name="upload" />音声ファイルを追加
            {/if}
            <input
              class="visually-hidden"
              type="file"
              accept="audio/*,.wav,.mp3,.flac,.ogg,.opus,.m4a,.aac,.webm"
              multiple
              disabled={uploading}
              onchange={addReferences}
            />
          </label>
          {#if uploadError}
            <p class="help error" role="alert">{uploadError}</p>
          {/if}
          <p class="help">
            自分の声、または本人の同意を得た声を使用してください。
          </p>
        </div>
      {/if}

      {#if usesCaption}
        <div class="field">
          <label for="caption">声・話し方の説明</label>
          <textarea
            id="caption"
            class="input"
            rows="3"
            placeholder="落ち着いた女性の声で、やわらかく丁寧に話す。"
            bind:value={form.caption}
            aria-describedby="caption-help"></textarea>
          <p id="caption-help" class="help">
            声の高さ・雰囲気・感情を短く指定します。
          </p>
        </div>
        <div class="field">
          <label for="caption-example">説明の入力例</label>
          <select
            id="caption-example"
            class="input"
            value=""
            onchange={(event) => {
              const select = event.currentTarget;
              if (select.value) {
                form.caption = select.value;
              }
              select.value = "";
            }}
          >
            <option value="">選ぶと説明欄に入力します</option>
            {#each CAPTION_EXAMPLES as example (example)}
              <option value={example}>{example}</option>
            {/each}
          </select>
        </div>
      {/if}

      <div class="check-row">
        <label class="check">
          <input type="checkbox" bind:checked={form.dictionary_enabled} />
          読み辞書を使う
        </label>
        <a href="/dictionary">読み辞書を編集</a>
      </div>
    </section>

    <section class="panel" aria-labelledby="step-2">
      <h2 id="step-2">② 文章・絵文字を入力</h2>
      <div class="field">
        <label for="text">読み上げる文章</label>
        <textarea
          id="text"
          class="input"
          rows="6"
          placeholder="どうもおばんでした。今日はどんな一日でしたか？"
          bind:value={form.text}
          bind:this={textArea}
          aria-describedby="text-help"></textarea>
        <p id="text-help" class="help">
          読み間違える漢字や名前は、読み辞書で登録できます。
        </p>
      </div>
      <p class="help emoji-help">
        挿入したい場所をクリックしてから絵文字を選んでください。選択中の文字は置き換えます。
      </p>
      {#if groups.length > 0}
        <div class="emoji-tabs" role="tablist" aria-label="絵文字の分類">
          {#each groups as group, index (group.title)}
            <button
              id={`emoji-tab-${index}`}
              type="button"
              role="tab"
              aria-selected={emojiTab === index}
              aria-controls="emoji-panel"
              tabindex={emojiTab === index ? 0 : -1}
              onclick={() => (emojiTab = index)}
              onkeydown={onTabKey}>{group.title}</button
            >
          {/each}
        </div>
        <div
          id="emoji-panel"
          class="emoji-panel"
          role="tabpanel"
          aria-labelledby={`emoji-tab-${emojiTab}`}
        >
          {#each groups[emojiTab]?.items ?? [] as item (item.label)}
            <button
              class="emoji"
              type="button"
              title={item.description}
              aria-label={`${item.label}の絵文字を挿入`}
              onpointerdown={(event) => event.preventDefault()}
              onclick={() => void insertEmoji(item.emoji)}
              >{item.emoji} {item.label}</button
            >
          {/each}
        </div>
      {:else if info.loading}
        <p class="help">
          <span class="spinner" aria-hidden="true"></span>絵文字を読み込み中…
        </p>
      {/if}
      <button class="btn" type="button" onclick={() => void showPreview()}>
        読み辞書を反映した文章を確認
      </button>
      {#if previewError}
        <p class="help error" role="alert">{previewError}</p>
      {:else if preview !== null}
        <div class="field">
          <label for="preview">読み上げに使う文章（確認用）</label>
          <textarea id="preview" class="input" rows="3" readonly value={preview}
          ></textarea>
        </div>
      {/if}
    </section>

    <section class="panel" aria-labelledby="step-3">
      <h2 id="step-3">③ 音声を生成・保存</h2>
      <button
        class="btn btn-primary generate"
        type="button"
        disabled={create.generating || !info.value}
        onclick={() => void generate()}
      >
        音声を生成する
      </button>
      <p
        class="status"
        class:error={create.statusKind === "error"}
        role="status"
        data-kind={create.statusKind}
      >
        {#if create.generating}<span class="spinner" aria-hidden="true"
          ></span>{/if}{create.status}
      </p>
      {#if create.generations.length > 1}
        <div class="field">
          <label for="candidate">聞き比べる音声</label>
          <select id="candidate" class="input" bind:value={create.selected}>
            {#each create.generations as generation, index (generation.id)}
              <option value={index}>音声 {index + 1}</option>
            {/each}
          </select>
        </div>
      {/if}
      {#if current}
        <audio
          class="player"
          controls
          src={current.audio_url}
          bind:volume={playback.volume}
          aria-label="できあがった音声"
        ></audio>
        <a class="btn download" href={downloadUrl(current)} download
          ><Icon name="download" />WAVをダウンロード</a
        >
      {/if}
      <div class="field">
        <label for="volume"
          >再生音量：<output for="volume"
            >{Math.round(playback.volume * 100)}</output
          >％</label
        >
        <input
          id="volume"
          type="range"
          min="0"
          max="100"
          step="1"
          value={Math.round(playback.volume * 100)}
          oninput={(event) =>
            saveVolume(Number(event.currentTarget.value) / 100)}
          aria-describedby="volume-help"
        />
        <p id="volume-help" class="help">
          このブラウザに記憶します。保存するWAVの音量は変更しません。
        </p>
      </div>
      <p class="help">
        生成した音声は<a href="/history">生成履歴</a>にも保存されます。
      </p>
    </section>
  </div>

  <details class="settings">
    <summary>生成設定</summary>
    <div class="settings-grid">
      <Slider
        label="生成の計算回数"
        help="多いほど時間がかかります。最初は40で試してください。"
        min={1}
        max={120}
        bind:value={form.num_steps}
      />
      <Slider
        label="生成する候補数"
        help="複数の声を作って聞き比べます。生成結果の選択欄で切り替えます。"
        min={1}
        max={info.value?.max_candidates ?? 32}
        bind:value={form.num_candidates}
      />
      <Slider
        label="話速（倍）"
        help="1.00が標準。声の高さを保って速度を変更し、WAVにも反映します。自然さを保つ目安は0.85～1.20です。変更後は再生成してください。"
        min={0.75}
        max={1.5}
        step={0.05}
        bind:value={form.speed}
      />
      <Slider
        label="生成する長さの倍率"
        help="1が標準。大きいほど長めに生成します。"
        min={0.5}
        max={1.5}
        step={0.01}
        bind:value={form.duration_scale}
      />
      <div class="field">
        <label for="seed">再現用の番号（シード）</label>
        <input
          id="seed"
          class="input"
          bind:value={form.seed}
          aria-describedby="seed-help"
        />
        <p id="seed-help" class="help">空欄なら毎回ランダムです。</p>
      </div>
      <div class="field">
        <label for="seconds">生成する秒数（任意）</label>
        <input
          id="seconds"
          class="input"
          bind:value={form.seconds}
          aria-describedby="seconds-help"
        />
        <p id="seconds-help" class="help">通常は空欄で自動推定します。</p>
      </div>
      <Slider
        label="文章への忠実さ"
        help="上げすぎると不自然になる場合があります。"
        min={0}
        max={10}
        step={0.1}
        bind:value={form.cfg_scale_text}
      />
      <Slider
        label="声の説明の反映度"
        help="上げすぎると不自然になる場合があります。"
        min={0}
        max={10}
        step={0.1}
        bind:value={form.cfg_scale_caption}
      />
      <Slider
        label="お手本の声の反映度"
        help="上げすぎると不自然になる場合があります。"
        min={0}
        max={10}
        step={0.1}
        bind:value={form.cfg_scale_speaker}
      />
    </div>
  </details>

  <details class="settings">
    <summary>詳細設定（通常は変更不要）</summary>
    <h3>機器・計算方式</h3>
    <div class="settings-grid">
      {#each [["model", "音声生成"], ["codec", "音声変換"]] as const as [which, label] (which)}
        <div class="field">
          <label for={`${which}-device`}>{label}に使う機器</label>
          {#if which === "model"}
            <select
              id="model-device"
              class="input"
              bind:value={form.model_device}
              onchange={() => changeDevice("model")}
              aria-describedby="device-help"
            >
              {#each info.value?.devices ?? [form.model_device] as device (device)}
                <option value={device}>{device}</option>
              {/each}
            </select>
          {:else}
            <select
              id="codec-device"
              class="input"
              bind:value={form.codec_device}
              onchange={() => changeDevice("codec")}
              aria-describedby="device-help"
            >
              {#each info.value?.devices ?? [form.codec_device] as device (device)}
                <option value={device}>{device}</option>
              {/each}
            </select>
          {/if}
        </div>
        <div class="field">
          <label for={`${which}-precision`}>{label}の計算精度</label>
          {#if which === "model"}
            <select
              id="model-precision"
              class="input"
              bind:value={form.model_precision}
            >
              {#each precisionsFor(form.model_device) as precision (precision)}
                <option value={precision}>{precision}</option>
              {/each}
            </select>
          {:else}
            <select
              id="codec-precision"
              class="input"
              bind:value={form.codec_precision}
            >
              {#each precisionsFor(form.codec_device) as precision (precision)}
                <option value={precision}>{precision}</option>
              {/each}
            </select>
          {/if}
        </div>
      {/each}
      <p id="device-help" class="help wide">
        cuda・xpuはGPU、cpuはCPUを使用します。計算精度は通常は初期値で使ってください。
      </p>
      <div class="field">
        <label for="schedule">計算の進め方</label>
        <select id="schedule" class="input" bind:value={form.t_schedule_mode}>
          <option value="linear">標準</option>
          <option value="sway">Sway（実験的）</option>
        </select>
      </div>
      <Slider
        label="Swayの調整値"
        help="Swayを選んだ場合だけ使います。"
        min={-1}
        max={1.5}
        step={0.1}
        disabled={form.t_schedule_mode !== "sway"}
        bind:value={form.sway_coeff}
      />
      <div class="field">
        <label for="guidance">条件を反映する方式</label>
        <select id="guidance" class="input" bind:value={form.cfg_guidance_mode}>
          <option value="independent">個別</option>
          <option value="joint">一括</option>
          <option value="alternating">交互</option>
        </select>
      </div>
      <div class="field">
        <label class="check">
          <input type="checkbox" bind:checked={form.context_kv_cache} />
          共通の計算結果を再利用する
        </label>
      </div>
      <div class="field wide">
        <button
          class="btn unload"
          type="button"
          disabled={create.generating || create.unloading}
          onclick={() => void unload()}
          aria-describedby="unload-help">モデルをメモリから解放</button
        >
        <p id="unload-help" class="help">
          ダウンロード済みファイルは残ります。次回生成時に再読み込みします。
        </p>
      </div>
    </div>
    <h3>内部補正・追加学習</h3>
    <div class="settings-grid">
      {#each RAW_FIELDS as [key, label] (key)}
        <div class="field">
          <label for={key}>{label}</label>
          <input
            id={key}
            class="input"
            bind:value={form[key]}
            aria-describedby="raw-help"
          />
        </div>
      {/each}
      <div class="field">
        <label for="cfg-min-t">条件反映の開始位置</label>
        <input
          id="cfg-min-t"
          class="input"
          type="number"
          step="0.01"
          bind:value={form.cfg_min_t}
          aria-describedby="cfg-min-help"
        />
        <p id="cfg-min-help" class="help">初期値は0.5です。</p>
      </div>
      <div class="field">
        <label for="cfg-max-t">条件反映の終了位置</label>
        <input
          id="cfg-max-t"
          class="input"
          type="number"
          step="0.01"
          bind:value={form.cfg_max_t}
          aria-describedby="cfg-max-help"
        />
        <p id="cfg-max-help" class="help">初期値は1です。</p>
      </div>
      <p id="raw-help" class="help wide">
        上の8項目は通常は空欄にしてください。
      </p>
    </div>
  </details>

  <details class="settings">
    <summary>実行記録（問い合わせ用）</summary>
    <div class="field">
      <label for="log">詳しい実行記録（一部英語）</label>
      <textarea id="log" class="input log" rows="10" readonly value={create.log}
      ></textarea>
    </div>
  </details>
</div>

<style lang="sass">
  .create
    max-width: 720px

  @media (min-width: 1100px)
    .create
      max-width: 1440px

    .panels
      grid-template-columns: 3fr 5fr 3fr

  .panels
    display: grid
    gap: var(--sp-4)
    align-items: start
    margin-bottom: var(--sp-4)

  .panel
    display: flex
    flex-direction: column
    gap: var(--sp-3)
    min-width: 0
    padding: var(--sp-4)
    border: 1px solid var(--c-border)
    border-radius: var(--radius-lg)
    background: var(--c-surface-raised)

    h2
      margin: 0
      font-size: var(--fs-xl)
      font-weight: 600
      line-height: 1.3

  .error-banner
    display: flex
    flex-wrap: wrap
    align-items: center
    justify-content: space-between
    gap: var(--sp-2)
    margin-bottom: var(--sp-4)

  .check-row
    display: flex
    flex-wrap: wrap
    align-items: center
    justify-content: space-between
    gap: var(--sp-2)
    font-size: var(--fs-sm)

  .reference-list
    display: flex
    flex-direction: column
    gap: var(--sp-1)
    margin: 0
    padding: 0
    list-style: none

    li
      display: flex
      align-items: center
      gap: var(--sp-1)
      padding-left: var(--sp-2)
      border: 1px solid var(--c-border)
      border-radius: var(--radius-sm)

    button:disabled
      opacity: 0.5
      cursor: default

  .reference-name
    flex: 1
    min-width: 0
    overflow: hidden
    text-overflow: ellipsis
    white-space: nowrap
    font-size: var(--fs-sm)

  .upload
    display: inline-flex
    align-items: center
    gap: var(--sp-2)
    align-self: flex-start

  .upload:focus-within
    outline: 2px solid var(--c-accent)
    outline-offset: 2px

  .emoji-help
    margin: 0

  .emoji-tabs
    display: flex
    flex-wrap: wrap
    gap: var(--sp-1)
    border-bottom: 1px solid var(--c-border)

    button
      padding: var(--sp-1) var(--sp-2)
      border: none
      border-bottom: 2px solid transparent
      background: transparent
      color: var(--c-muted)
      font-size: var(--fs-sm)
      cursor: pointer

      &[aria-selected="true"]
        border-bottom-color: var(--c-accent)
        color: var(--c-on-surface)
        font-weight: 600

  .emoji-panel
    display: flex
    flex-wrap: wrap
    gap: 6px

  .emoji
    padding: var(--sp-1) var(--sp-2)
    border: 1px solid var(--c-border)
    border-radius: var(--radius-sm)
    background: var(--c-surface)
    color: var(--c-on-surface)
    font-size: var(--fs-sm)
    cursor: pointer

    &:hover
      background: var(--c-hover-1)

  .generate
    min-height: 44px

  .status
    margin: 0
    font-size: var(--fs-sm)
    color: var(--c-muted)

    &.error
      color: var(--c-danger)

  .player
    width: 100%

  .download
    display: inline-flex
    align-items: center
    justify-content: center
    gap: var(--sp-2)
    text-decoration: none

  #volume
    width: 100%
    accent-color: var(--c-accent)

  .settings
    margin-bottom: var(--sp-3)
    border: 1px solid var(--c-border)
    border-radius: var(--radius-lg)
    background: var(--c-surface-raised)

    summary
      padding: var(--sp-3) var(--sp-4)
      font-size: var(--fs-md)
      font-weight: 500
      cursor: pointer

    h3
      margin: var(--sp-2) var(--sp-4) 0
      font-size: var(--fs-md)
      font-weight: 600

    > .field
      padding: 0 var(--sp-4) var(--sp-4)

  .settings-grid
    display: grid
    gap: var(--sp-3) var(--sp-5)
    padding: var(--sp-3) var(--sp-4) var(--sp-4)

    .wide
      grid-column: 1 / -1

  @media (min-width: 768px)
    .settings-grid
      grid-template-columns: 1fr 1fr

  .unload
    align-self: flex-start

  .log
    font-family: ui-monospace, monospace
    font-size: var(--fs-xs)
</style>
