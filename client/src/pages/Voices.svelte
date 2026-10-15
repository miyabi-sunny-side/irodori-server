<script lang="ts">
  import { tick } from "svelte";

  import {
    api,
    ApiError,
    type Generation,
    type Reference,
    type Voice,
  } from "../lib/api";
  import ConfirmModal from "../lib/ConfirmModal.svelte";
  import { defaultRequest, trimRequest } from "../lib/create";
  import Icon from "../lib/Icon.svelte";
  import { info, loadInfo, modelLabel, playback } from "../lib/state.svelte";
  import { referenceNote, VOICE_NAME_MAX, voiceName } from "../lib/voices";

  // EasyIrodoriTTS v2.0 voices every registration candidate with one text.
  const SAMPLE_TEXT =
    "おはようございます。今日は空が晴れて気持ちのよい朝ですね。窓を開けると涼しい風が入ってきました。温かいお茶を飲んでゆっくり出かける準備をしましょう。";
  const VOICE_EXAMPLES = [
    [
      "女性・明るい",
      "明るく親しみやすい女性の声。自然な高さで、はっきりと話す。",
    ],
    [
      "女性・落ち着いた",
      "落ち着いた柔らかな女性の声。穏やかで、聞き取りやすく話す。",
    ],
    ["男性・明るい", "明るく爽やかな男性の声。自然な高さで、歯切れよく話す。"],
    [
      "男性・落ち着いた",
      "落ち着いた低めの男性の声。温かみがあり、丁寧に話す。",
    ],
  ];
  const STYLE_EXAMPLES = [
    ["明るく", "明るく、親しみやすい口調で話す。"],
    ["穏やかに", "穏やかで、やさしい口調で話す。"],
    [
      "丁寧なナレーション",
      "聞き取りやすい速さで、はっきりと丁寧に読み上げる。",
    ],
    ["指定なし", ""],
  ];
  const METHODS = [
    ["design", "新しい声を作る"],
    ["direct", "手元の音声をそのまま使う"],
    ["both", "お手本＋話し方で作る"],
  ] as const;

  type Method = (typeof METHODS)[number][0];

  let method = $state<Method>("design");
  let model = $state("Aratako/Irodori-TTS-v4.1-Small");
  let description = $state("");
  let style = $state("");
  let upload = $state<Reference | null>(null);
  let uploadSeconds = $state(Number.NaN);
  let uploading = $state(false);
  let candidates = $state<Generation[]>([]);
  let candidateCount = $state(3);
  let picked = $state<number | null>(null);
  let testText = $state("こんにちは。今日はどんな一日でしたか？");
  let testAudio = $state<Generation | null>(null);
  let working = $state(false);
  let name = $state("");
  let formError = $state("");
  let message = $state("");
  let progress = $state("");

  let voices = $state<Voice[]>([]);
  let listState = $state<"loading" | "empty" | "error" | "success">("loading");
  let renaming = $state<Voice | null>(null);
  let newName = $state("");
  let renameError = $state("");
  let deleting = $state<Voice | null>(null);
  let returnFocus: HTMLElement | null = null;

  let note = $derived(referenceNote(uploadSeconds));
  let ready = $derived(
    method === "direct"
      ? upload !== null && !note.error
      : picked !== null && candidates.some((g) => g.id === picked),
  );

  function resetSource() {
    candidates = [];
    picked = null;
    testAudio = null;
    formError = "";
  }

  async function loadVoices() {
    try {
      voices = (await api.voices()).voices;
      listState = voices.length === 0 ? "empty" : "success";
    } catch {
      listState = "error";
    }
  }

  async function chooseFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    resetSource();
    upload = null;
    uploadSeconds = Number.NaN;
    uploading = true;
    try {
      upload = await api.uploadReference(file);
    } catch (error) {
      formError =
        error instanceof ApiError
          ? error.message
          : "音声ファイルを追加できませんでした。";
    } finally {
      uploading = false;
    }
  }

  async function run(text: string, count: number, test: boolean) {
    formError = "";
    working = true;
    progress = test
      ? "テスト音声を作成しています…"
      : "候補を作成しています…初回はモデルを取得するため時間がかかります。";
    const mode =
      method === "design"
        ? description.trim()
          ? "design"
          : "auto"
        : (test ? style : description).trim()
          ? "both"
          : "clone";
    try {
      const result = await api.generate(
        trimRequest({
          ...defaultRequest(),
          model,
          mode,
          caption: test ? style : description,
          reference_ids: upload ? [upload.id] : [],
          text,
          dictionary_enabled: false,
          num_candidates: count,
        }),
      );
      if (test) {
        testAudio = result.generations[0] ?? null;
      } else {
        candidates = result.generations;
        picked = candidates[0]?.id ?? null;
      }
    } catch (error) {
      formError =
        error instanceof ApiError ? error.message : "作成できませんでした。";
    } finally {
      working = false;
      progress = "";
    }
  }

  async function register(event: SubmitEvent) {
    event.preventDefault();
    const checked = voiceName(name);
    if ("error" in checked) {
      formError = checked.error;
      return;
    }
    working = true;
    formError = "";
    try {
      const voice = await api.addVoice({
        name: checked.value,
        model,
        caption: style,
        ...(method === "direct"
          ? { reference_id: upload!.id }
          : { generation_id: picked! }),
      });
      message = `「${voice.name}」を登録しました。`;
      name = "";
      resetSource();
      await loadVoices();
    } catch (error) {
      formError =
        error instanceof ApiError ? error.message : "登録できませんでした。";
    } finally {
      working = false;
    }
  }

  function startRename(voice: Voice, event: MouseEvent) {
    returnFocus = event.currentTarget as HTMLElement;
    renaming = voice;
    newName = voice.name;
    renameError = "";
    void tick().then(() => document.getElementById("rename")?.focus());
  }

  async function confirmRename(event: SubmitEvent) {
    event.preventDefault();
    const voice = renaming;
    const checked = voiceName(newName);
    if (!voice) return;
    if ("error" in checked) {
      renameError = checked.error;
      return;
    }
    try {
      await api.renameVoice(voice, checked.value);
      renaming = null;
      message = `名前を「${checked.value}」に変更しました。`;
      await loadVoices();
      returnFocus?.focus();
    } catch (error) {
      renameError =
        error instanceof ApiError ? error.message : "変更できませんでした。";
      // After another screen's change, retry against the voice as stored now.
      if (error instanceof ApiError && error.message.includes("読み直して")) {
        await loadVoices();
        renaming = voices.find((v) => v.id === voice.id) ?? null;
      }
    }
  }

  async function confirmDelete() {
    const voice = deleting;
    deleting = null;
    if (!voice) return;
    try {
      await api.deleteVoice(voice.id);
      message = `「${voice.name}」の登録を削除しました。`;
      await loadVoices();
    } catch (error) {
      message =
        error instanceof ApiError ? error.message : "削除できませんでした。";
    }
  }

  $effect(() => {
    void loadInfo();
    void loadVoices();
  });
</script>

<div class="content voices">
  <section class="panel" aria-labelledby="register-title">
    <h2 id="register-title">声を登録する</h2>
    <form class="form" onsubmit={(event) => void register(event)}>
      <fieldset class="methods">
        <legend class="label">作り方</legend>
        {#each METHODS as [value, label] (value)}
          <label class="radio" class:checked={method === value}>
            <input
              type="radio"
              name="method"
              {value}
              checked={method === value}
              onchange={() => {
                method = value;
                resetSource();
              }}
            />
            {label}
          </label>
        {/each}
      </fieldset>

      <div class="field">
        <label for="voice-model">使用モデル</label>
        <select
          id="voice-model"
          class="input"
          bind:value={model}
          onchange={resetSource}
        >
          {#each info.value?.models ?? [] as option (option.id)}
            <option value={option.id}>{option.label}</option>
          {/each}
        </select>
      </div>

      {#if method !== "design"}
        <div class="field">
          <span class="label" id="upload-label">お手本の音声</span>
          <label class="btn upload" class:busy={uploading}>
            {#if uploading}
              <span class="spinner" aria-hidden="true"></span>追加しています…
            {:else}
              <Icon name="upload" />音声ファイルを選ぶ
            {/if}
            <input
              class="visually-hidden"
              type="file"
              accept="audio/*,.wav,.mp3,.flac,.ogg,.opus,.m4a,.aac,.webm"
              aria-labelledby="upload-label"
              disabled={uploading}
              onchange={(event) => void chooseFile(event)}
            />
          </label>
          {#if upload}
            <div class="audio-row">
              <span class="file-name">{upload.name}</span>
              <audio
                controls
                preload="metadata"
                src={upload.audio_url}
                bind:volume={playback.volume}
                onloadedmetadata={(event) =>
                  (uploadSeconds = event.currentTarget.duration)}
                aria-label="お手本の音声"
              ></audio>
            </div>
          {/if}
          {#if note.error}
            <p class="help error" role="alert">{note.error}</p>
          {:else if note.warning}
            <p class="help warning">{note.warning}</p>
          {/if}
          <p class="help">
            1～120秒。自分の声、または本人の同意を得た音声を使ってください。
          </p>
        </div>
      {/if}

      {#if method !== "direct"}
        <div class="field">
          <label for="description"
            >{method === "design" ? "声・話し方の説明" : "話し方の指定"}</label
          >
          <textarea
            id="description"
            class="input"
            rows="2"
            placeholder={method === "design"
              ? "空欄でおまかせ"
              : "空欄でお手本のまま"}
            bind:value={description}
            oninput={resetSource}></textarea>
          <select
            class="input example"
            aria-label="説明の入力例から選ぶ"
            value=""
            onchange={(event) => {
              const picked = event.currentTarget.value;
              if (picked) {
                description = picked;
                resetSource();
              }
              event.currentTarget.value = "";
            }}
          >
            <option value="">入力例から選ぶ…</option>
            {#each method === "design" ? VOICE_EXAMPLES : STYLE_EXAMPLES.slice(0, 3) as [label, text] (label)}
              <option value={text}>{label}</option>
            {/each}
          </select>
        </div>
        <div class="candidates-head">
          <label class="small">
            候補数
            <select class="input count" bind:value={candidateCount}>
              {#each [1, 2, 3, 4, 5, 6, 7, 8] as n (n)}
                <option value={n}>{n}</option>
              {/each}
            </select>
          </label>
          <button
            class="btn"
            type="button"
            disabled={working || (method === "both" && !upload)}
            onclick={() => void run(SAMPLE_TEXT, candidateCount, false)}
            >候補の音声を作成</button
          >
        </div>
        {#if candidates.length > 0}
          <fieldset class="candidate-list">
            <legend class="label">登録する候補</legend>
            {#each candidates as candidate, index (candidate.id)}
              <label class="candidate" class:checked={picked === candidate.id}>
                <input
                  type="radio"
                  name="candidate"
                  checked={picked === candidate.id}
                  onchange={() => (picked = candidate.id)}
                />
                <span class="candidate-name">候補 {index + 1}</span>
                <audio
                  controls
                  preload="none"
                  src={candidate.audio_url}
                  bind:volume={playback.volume}
                  aria-label={`候補 ${index + 1}`}
                ></audio>
              </label>
            {/each}
          </fieldset>
        {/if}
      {/if}

      <div class="field">
        <label for="style">登録後の話し方（任意）</label>
        <textarea
          id="style"
          class="input"
          rows="2"
          bind:value={style}
          aria-describedby="style-help"></textarea>
        <select
          class="input example"
          aria-label="話し方の入力例から選ぶ"
          value=""
          onchange={(event) => {
            const index = Number(event.currentTarget.value);
            if (event.currentTarget.value !== "")
              style = STYLE_EXAMPLES[index][1];
            event.currentTarget.value = "";
          }}
        >
          <option value="">入力例から選ぶ…</option>
          {#each STYLE_EXAMPLES as [label], index (label)}
            <option value={String(index)}>{label}</option>
          {/each}
        </select>
        <p id="style-help" class="help">
          この声でセリフを作るときの話し方です。
        </p>
      </div>

      {#if method === "direct" && upload && !note.error}
        <details class="test">
          <summary>テスト音声を作る（任意）</summary>
          <div class="test-body">
            <input
              class="input"
              aria-label="テスト用の短い文章"
              bind:value={testText}
            />
            <button
              class="btn"
              type="button"
              disabled={working || !testText.trim()}
              onclick={() => void run(testText, 1, true)}
              >テスト音声を作成</button
            >
            {#if testAudio}
              <audio
                controls
                src={testAudio.audio_url}
                bind:volume={playback.volume}
                aria-label="テスト音声"
              ></audio>
            {/if}
          </div>
        </details>
      {/if}

      <div class="field">
        <label for="voice-name">話者名</label>
        <input
          id="voice-name"
          class="input"
          maxlength={VOICE_NAME_MAX}
          autocomplete="off"
          bind:value={name}
        />
      </div>
      <p class="status" role="status">
        {#if working}<span class="spinner" aria-hidden="true"
          ></span>{progress}{/if}
      </p>
      {#if formError}
        <p class="help error" role="alert">{formError}</p>
      {/if}
      <button
        class="btn btn-primary register"
        type="submit"
        disabled={!ready || working}>この音声を登録する</button
      >
    </form>
  </section>

  <section aria-labelledby="list-title" data-state={listState}>
    <h2 id="list-title" class="list-title">登録した声</h2>
    <p class="message" role="status">{message}</p>
    {#if listState === "loading"}
      <p class="state">
        <span class="spinner" aria-hidden="true"></span>読み込み中…
      </p>
    {:else if listState === "empty"}
      <p class="state">まだ登録した声はありません。</p>
    {:else if listState === "error"}
      <div class="state-wrap">
        <p class="state error">登録した声を読み込めませんでした。</p>
        <button class="btn" type="button" onclick={() => void loadVoices()}
          >再読み込み</button
        >
      </div>
    {:else}
      <ul class="list">
        {#each voices as voice (voice.id)}
          <li class="card">
            <div class="card-head">
              {#if renaming?.id === voice.id}
                <form
                  class="rename"
                  onsubmit={(event) => void confirmRename(event)}
                >
                  <input
                    id="rename"
                    class="input"
                    aria-label="新しい話者名"
                    maxlength={VOICE_NAME_MAX}
                    aria-invalid={renameError ? "true" : undefined}
                    bind:value={newName}
                  />
                  <button class="btn" type="submit">変更</button>
                  <button
                    class="btn"
                    type="button"
                    onclick={() => {
                      renaming = null;
                      returnFocus?.focus();
                    }}>キャンセル</button
                  >
                </form>
              {:else}
                <span class="name">{voice.name}</span>
                <span class="tools">
                  <button
                    class="icon-btn"
                    type="button"
                    aria-label={`${voice.name}の名前を変更`}
                    title="名前を変更"
                    onclick={(event) => startRename(voice, event)}
                    ><Icon name="pencil" /></button
                  >
                  <button
                    class="icon-btn"
                    type="button"
                    aria-label={`${voice.name}を削除`}
                    title="削除"
                    onclick={(event) => {
                      returnFocus = event.currentTarget;
                      deleting = voice;
                    }}><Icon name="trash" /></button
                  >
                </span>
              {/if}
            </div>
            {#if renaming?.id === voice.id && renameError}
              <p class="help error" role="alert">{renameError}</p>
            {/if}
            <p class="meta">
              {modelLabel(voice.model)}{#if voice.caption}・{voice.caption}{/if}
            </p>
            <audio
              controls
              preload="none"
              src={voice.audio_url}
              bind:volume={playback.volume}
              aria-label={`${voice.name}のお手本の音声`}
            ></audio>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

{#if deleting}
  <ConfirmModal
    title="話者の削除"
    confirmLabel="削除する"
    onconfirm={() => void confirmDelete()}
    oncancel={() => {
      deleting = null;
      returnFocus?.focus();
    }}
  >
    「{deleting.name}」の登録を削除しますか？使っているセリフは話者の選び直しが必要です。生成した音声は生成履歴に残ります。
  </ConfirmModal>
{/if}

<style lang="sass">
  .voices
    display: flex
    flex-direction: column
    gap: var(--sp-5)

  .panel
    padding: var(--sp-4)
    border: 1px solid var(--c-border)
    border-radius: var(--radius-lg)
    background: var(--c-surface-raised)

  h2
    margin: 0 0 var(--sp-3)
    font-size: var(--fs-xl)
    font-weight: 600
    line-height: 1.3

  .form
    display: flex
    flex-direction: column
    gap: var(--sp-3)

  fieldset
    display: flex
    flex-wrap: wrap
    gap: var(--sp-2)
    margin: 0
    padding: 0
    border: none

    legend
      margin-bottom: var(--sp-1)

  .radio
    display: inline-flex
    align-items: center
    gap: var(--sp-2)
    padding: var(--sp-1) var(--sp-2)
    border-radius: var(--radius-sm)
    font-size: var(--fs-sm)
    cursor: pointer

    &.checked
      background: var(--c-accent-subtle)

    input
      accent-color: var(--c-accent)

  .upload
    display: inline-flex
    align-items: center
    gap: var(--sp-2)
    align-self: flex-start

  .upload:focus-within
    outline: 2px solid var(--c-accent)
    outline-offset: 2px

  .audio-row
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2)

    audio
      flex: 1 1 220px
      min-width: 0
      height: 40px

  .file-name
    font-size: var(--fs-sm)
    overflow-wrap: anywhere

  .help.warning
    color: var(--c-on-surface)

  .example
    max-width: 320px

  .candidates-head
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-3)

  .small
    display: inline-flex
    align-items: center
    gap: var(--sp-2)
    font-size: var(--fs-sm)

  .count
    width: auto

  .candidate-list
    flex-direction: column

  .candidate
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2)
    padding: var(--sp-1) var(--sp-2)
    border-radius: var(--radius-sm)

    &.checked
      background: var(--c-accent-subtle)

    input
      accent-color: var(--c-accent)

    audio
      flex: 1 1 200px
      min-width: 0
      height: 40px

  .candidate-name
    font-size: var(--fs-sm)

  .test
    summary
      font-size: var(--fs-sm)
      cursor: pointer

  .test-body
    display: flex
    flex-direction: column
    gap: var(--sp-2)
    margin-top: var(--sp-2)

  .status
    margin: 0
    font-size: var(--fs-sm)
    color: var(--c-muted)

    &:empty
      display: none

  .register
    min-height: 44px

  .list-title
    margin-bottom: var(--sp-2)

  .message
    margin: 0 0 var(--sp-2)
    font-size: var(--fs-sm)

    &:empty
      display: none

  .list
    display: flex
    flex-direction: column
    gap: var(--sp-2)
    margin: 0
    padding: 0
    list-style: none

  .card
    display: flex
    flex-direction: column
    gap: var(--sp-1)
    padding: 10px
    border: 1px solid var(--c-border)
    border-radius: var(--radius-md)
    background: var(--c-surface-raised)

    audio
      width: 100%
      height: 40px

  .card-head
    display: flex
    align-items: center
    justify-content: space-between
    gap: var(--sp-2)
    margin: -4px -4px 0 0

  .name
    min-width: 0
    font-weight: 600
    overflow-wrap: anywhere

  .tools
    display: flex
    flex: none

  .rename
    display: flex
    flex-wrap: wrap
    gap: var(--sp-2)
    width: 100%

    .input
      flex: 1 1 200px
      width: auto

  .meta
    display: -webkit-box
    margin: 0
    overflow: hidden
    -webkit-line-clamp: 2
    -webkit-box-orient: vertical
    font-size: var(--fs-xs)
    color: var(--c-muted)
    overflow-wrap: anywhere
</style>
