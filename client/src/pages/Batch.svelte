<script module lang="ts">
  // The form survives page changes, like 音声作成 (DESIGN.md, まとめて生成).
  const form = $state({
    character: "",
    mode: "clone",
    caption: "",
    lines: "",
    num_steps: 40,
    speed: 1,
  });
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";

  import { api, ApiError, type Batch, type Reference } from "../lib/api";
  import {
    characterOptions,
    lineSummary,
    MAX_BATCH_LINES,
    progressText,
  } from "../lib/batch";
  import Slider from "../lib/Slider.svelte";
  import { info, loadInfo } from "../lib/state.svelte";

  type ListState = "loading" | "empty" | "error" | "success";
  const POLL_MS = 1500;

  let listState = $state<ListState>("loading");
  let references = $state<Reference[]>([]);
  let batch = $state<Batch | null>(null);
  let startError = $state("");
  let starting = $state(false);
  let touched = $state(false);

  let characters = $derived(characterOptions(references));
  let summary = $derived(lineSummary(form.lines));
  let running = $derived(batch !== null && !batch.finished);
  let modes = $derived(
    (info.value?.modes ?? []).filter((mode) =>
      ["clone", "both"].includes(mode.id),
    ),
  );

  let alive = true;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    try {
      const [loaded, recent] = await Promise.all([
        api.references(),
        api.batches(),
      ]);
      references = loaded.references;
      if (!characters.some((option) => option.name === form.character)) {
        form.character = characters[0]?.name ?? "";
      }
      listState = characters.length === 0 ? "empty" : "success";
      batch = recent.batches[0] ?? null;
      if (running) schedule();
    } catch {
      listState = "error";
    }
  }

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(() => void poll(), POLL_MS);
  }

  async function poll() {
    if (!alive || !batch) return;
    try {
      batch = await api.batch(batch.id);
    } catch {
      // A missed poll is retried; progress is also kept by the server.
    }
    if (running) schedule();
  }

  async function start(event: SubmitEvent) {
    event.preventDefault();
    touched = true;
    startError = "";
    if (summary.error || !form.character) return;
    starting = true;
    try {
      const { id } = await api.startBatch(form.character, form.lines, {
        mode: form.mode,
        caption: form.mode === "both" ? form.caption : "",
        num_steps: form.num_steps,
        speed: form.speed,
      });
      batch = await api.batch(id);
      if (running) schedule();
      // The start button is now disabled; keep keyboard focus on the progress.
      await tick();
      document.getElementById("batch-progress-title")?.focus();
    } catch (error) {
      startError =
        error instanceof ApiError
          ? error.message
          : "まとめて生成を始められませんでした。";
    } finally {
      starting = false;
    }
  }

  // onMount, not $effect: loading must not re-run (and stop polling) when info arrives.
  onMount(() => {
    void loadInfo();
    void load();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  });
</script>

<div class="content" data-state={listState}>
  {#if listState === "loading"}
    <p class="state">
      <span class="spinner" aria-hidden="true"></span>読み込み中…
    </p>
  {:else if listState === "error"}
    <div class="state-wrap">
      <p class="state error">お手本を読み込めませんでした。</p>
      <button class="btn" type="button" onclick={() => void load()}>
        再読み込み
      </button>
    </div>
  {:else if listState === "empty"}
    <div class="state-wrap">
      <p class="state">
        まだお手本がありません。生成履歴で気に入った音声を「お手本に保存」すると、そのキャラクターの台詞をまとめて作れます。
      </p>
      <a href="/history">生成履歴へ</a>
    </div>
  {:else}
    <form class="batch-form" onsubmit={(event) => void start(event)}>
      <div class="field">
        <label for="batch-character">キャラクター</label>
        <select
          id="batch-character"
          class="input"
          bind:value={form.character}
          aria-describedby="batch-character-help"
        >
          {#each characters as option (option.name)}
            <option value={option.name}
              >{option.name}（お手本 {option.count}件）</option
            >
          {/each}
        </select>
        <p class="help" id="batch-character-help">
          このキャラクターのお手本をすべて使って、声を似せます。
        </p>
      </div>

      <div class="field">
        <label for="batch-mode">声の作り方</label>
        <select id="batch-mode" class="input" bind:value={form.mode}>
          {#each modes as mode (mode.id)}
            <option value={mode.id}>{mode.label}</option>
          {/each}
        </select>
      </div>

      {#if form.mode === "both"}
        <div class="field">
          <label for="batch-caption">声・話し方の説明</label>
          <textarea
            id="batch-caption"
            class="input"
            rows="3"
            placeholder="落ち着いた女性の声で、やわらかく丁寧に話す。"
            aria-describedby="batch-caption-help"
            bind:value={form.caption}></textarea>
          <p class="help" id="batch-caption-help">
            声の高さ・雰囲気・感情を短く指定します。
          </p>
        </div>
      {/if}

      <div class="field">
        <label for="batch-lines">読み上げる台詞（1行に1本）</label>
        <textarea
          id="batch-lines"
          class="input"
          rows="8"
          placeholder={"おはようございます。\n今日もよろしくお願いします。"}
          aria-describedby="batch-lines-help"
          aria-invalid={touched && summary.error ? "true" : undefined}
          bind:value={form.lines}></textarea>
        <p class="help" id="batch-lines-help">
          {summary.count}行（{MAX_BATCH_LINES}行まで）。空行は飛ばします。1行ごとに1本の音声を作り、生成履歴に送ります。
        </p>
        {#if touched && summary.error}
          <p class="help error" role="alert">{summary.error}</p>
        {/if}
      </div>

      <div class="sliders">
        <Slider
          label="生成の計算回数"
          help="多いほど時間がかかります。最初は40で試してください。"
          min={1}
          max={120}
          bind:value={form.num_steps}
        />
        <Slider
          label="話速（倍）"
          help="1.00が標準。声の高さを保って速度を変えます。"
          min={0.75}
          max={1.5}
          step={0.05}
          bind:value={form.speed}
        />
      </div>

      {#if startError}
        <p class="error-banner" role="alert">{startError}</p>
      {/if}
      <button
        class="btn btn-primary start"
        type="submit"
        disabled={running || starting}>まとめて生成する</button
      >
      {#if running}
        <p class="help">生成中はほかのまとめて生成を始められません。</p>
      {/if}
    </form>
  {/if}

  {#if batch}
    <section class="progress" aria-labelledby="batch-progress-title">
      <h2 id="batch-progress-title" tabindex="-1">
        {batch.character} のまとめて生成
      </h2>
      <progress max={batch.total} value={batch.done}></progress>
      <p class="status" role="status">
        {#if running}<span class="spinner" aria-hidden="true"
          ></span>{/if}{progressText(batch)}
      </p>
      <p class="help">
        ブラウザを閉じても生成は続きます。できた音声は生成履歴に送られます。
      </p>
      {#if batch.failed.length > 0}
        <div class="error-banner">
          <p>生成できなかった台詞（{batch.failed.length}件）</p>
          <ul>
            {#each batch.failed as failure, index (index)}
              <li><span class="line">{failure.line}</span>：{failure.error}</li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if batch.finished}
        <a href={`/history?character=${encodeURIComponent(batch.character)}`}
          >生成履歴で{batch.character}の音声を見る</a
        >
      {/if}
    </section>
  {/if}
</div>

<style lang="sass">
  .batch-form
    display: flex
    flex-direction: column
    gap: var(--sp-4)

  .sliders
    display: grid
    gap: var(--sp-4)

  @media (min-width: 768px)
    .sliders
      grid-template-columns: 1fr 1fr

  .start
    width: 100%
    height: 44px

  .progress
    display: flex
    flex-direction: column
    gap: var(--sp-2)
    margin-top: var(--sp-5)
    padding-top: var(--sp-4)
    border-top: 1px solid var(--c-border)

    h2
      margin: 0
      font-size: var(--fs-lg)
      font-weight: 600
      overflow-wrap: anywhere

    progress
      width: 100%
      height: 8px
      accent-color: var(--c-accent)

  .status
    margin: 0
    font-size: var(--fs-sm)

  .error-banner
    margin: 0

    p
      margin: 0 0 var(--sp-1)

    ul
      margin: 0
      padding-left: var(--sp-4)

  .line
    overflow-wrap: anywhere
</style>
