<script lang="ts">
  import {
    api,
    ApiError,
    type CardView,
    type DictionaryEntry,
    type Script,
    type ScriptState,
    type Voice,
  } from "../lib/api";
  import { defaultRequest } from "../lib/create";
  import GenerationSettings from "../lib/GenerationSettings.svelte";
  import Icon from "../lib/Icon.svelte";
  import Modal from "../lib/Modal.svelte";
  import ConfirmModal from "../lib/ConfirmModal.svelte";
  import ScriptCard from "../lib/ScriptCard.svelte";
  import {
    estimateSeconds,
    isLong,
    makeCard,
    MAX_CARDS,
    MAX_FILE_BYTES,
    packScriptFile,
    parseScriptFile,
    pushHistory,
    reorder,
    splitText,
  } from "../lib/script";
  import { info, loadInfo, playback, saveVolume } from "../lib/state.svelte";

  const SAVE_DELAY = 700;
  const POLL_INTERVAL = 1500;
  const TOKENS = ["$連番$", "$話者名$", "$テキスト$", "$日付$"];

  let server = $state<ScriptState | null>(null);
  let script = $state<Script | null>(null);
  let voices = $state<Voice[]>([]);
  let dictionary = $state<DictionaryEntry[]>([]);
  let loadError = $state("");
  let selected = $state("");
  let message = $state("");
  let messageKind = $state<"idle" | "busy" | "error">("idle");
  let conflict = $state(false);
  let undoStack = $state<Script[]>([]);
  let redoStack = $state<Script[]>([]);
  let confirmLong = $state<{ target: string; cards: number[] } | null>(null);
  let confirmReset = $state(false);
  let unloading = $state(false);
  let playingId = $state("");
  let player = $state<HTMLAudioElement | undefined>();
  let queue: { id: string; audio_url: string }[] = [];
  let dragFrom = -1;

  // Edits are saved after a pause; `edits` counts them so a save that
  // overlaps further typing leaves the newer edits pending. Each save is
  // one undo step back to the script as it was last stored.
  let edits = 0;
  let savedEdits = 0;
  let lastSaved: Script | null = null;
  let travelling = false;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let saving: Promise<void> | null = null;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;

  let views = $derived(
    new Map<string, CardView>((server?.cards ?? []).map((c) => [c.id, c])),
  );
  let job = $derived(server?.job ?? null);
  let running = $derived(job !== null && !job.finished);
  let readings = $derived(script?.dictionary_enabled ? dictionary : []);
  let scale = $derived(script?.settings.duration_scale ?? 1);

  const snapshot = () => $state.snapshot(script) as Script;

  function say(text: string, kind: "idle" | "busy" | "error" = "idle") {
    message = text;
    messageKind = kind;
  }

  function fail(error: unknown, fallback: string) {
    if (error instanceof ApiError && error.message.startsWith("別の画面で")) {
      conflict = true;
    }
    say(error instanceof ApiError ? error.message : fallback, "error");
  }

  function adopt(state: ScriptState, takeScript: boolean) {
    server = state;
    if (takeScript) {
      // Fields an older server did not store take the current defaults.
      state.script.settings = { ...defaultRequest(), ...state.script.settings };
      script = state.script;
      lastSaved = $state.snapshot(state.script) as Script;
      if (!state.script.cards.some((c) => c.id === selected)) {
        selected = state.script.cards[0]?.id ?? "";
      }
    }
    schedulePoll();
  }

  async function load() {
    loadError = "";
    try {
      const [state, voiceList, words] = await Promise.all([
        api.script(),
        api.voices(),
        api.dictionary(),
      ]);
      voices = voiceList.voices;
      dictionary = words.entries;
      edits = savedEdits = 0;
      conflict = false;
      adopt(state, true);
    } catch (error) {
      loadError =
        error instanceof ApiError
          ? error.message
          : "台本を読み込めませんでした。";
    }
  }

  /** Records an edit to the local script and saves it after a pause. */
  function edited() {
    edits += 1;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), SAVE_DELAY);
  }

  /** A structural change (cards added, removed, moved) is saved as its own undo step. */
  async function change(apply: (script: Script) => void) {
    if (!script) return;
    await save();
    apply(script);
    edits += 1;
    await save();
  }

  async function save(): Promise<void> {
    clearTimeout(saveTimer);
    if (saving) {
      await saving;
    }
    if (!script || !server || edits === savedEdits || conflict) return;
    const target = edits;
    const sent = snapshot();
    saving = (async () => {
      try {
        const state = await api.saveScript(server!.revision, sent);
        if (lastSaved && !travelling) {
          undoStack = pushHistory(undoStack, lastSaved);
          redoStack = [];
        }
        lastSaved = sent;
        savedEdits = target;
        adopt(state, false);
        if (messageKind === "error" && !conflict) say("");
      } catch (error) {
        fail(error, "台本を保存できませんでした。");
      }
    })();
    await saving;
    saving = null;
    if (edits !== savedEdits && !conflict && messageKind !== "error") {
      saveTimer = setTimeout(() => void save(), SAVE_DELAY);
    }
  }

  async function travel(back: boolean) {
    await save();
    const target = (back ? undoStack : redoStack).at(-1);
    if (!script || !target || !lastSaved || edits !== savedEdits) return;
    if (back) {
      undoStack = undoStack.slice(0, -1);
      redoStack = pushHistory(redoStack, lastSaved);
    } else {
      redoStack = redoStack.slice(0, -1);
      undoStack = pushHistory(undoStack, lastSaved);
    }
    script = $state.snapshot(target) as Script;
    if (!script.cards.some((c) => c.id === selected)) {
      selected = script.cards[0].id;
    }
    edits += 1;
    travelling = true;
    await save();
    travelling = false;
  }

  function schedulePoll() {
    clearTimeout(pollTimer);
    if (server?.job && !server.job.finished) {
      pollTimer = setTimeout(() => void poll(), POLL_INTERVAL);
    }
  }

  async function poll() {
    try {
      const state = await api.script();
      const unsaved = edits !== savedEdits || saving !== null;
      adopt(state, !unsaved && state.revision !== server?.revision);
      if (state.job?.finished) {
        say(
          state.job.failed > 0
            ? `生成が終わりました（成功 ${state.job.done - state.job.failed}件・失敗 ${state.job.failed}件）。`
            : `生成が終わりました（${state.job.done}件）。`,
          state.job.failed > 0 ? "error" : "idle",
        );
      }
    } catch {
      schedulePoll();
    }
  }

  function targets(target: string): number[] {
    if (!script) return [];
    return script.cards.flatMap((card, index) => {
      const status = views.get(card.id)?.status;
      const wanted =
        target === "all"
          ? true
          : target === "changed"
            ? status !== "done"
            : card.id === target;
      return wanted && card.text.trim() && isLong(card.text, readings, scale)
        ? [index + 1]
        : [];
    });
  }

  async function generate(target: string, confirmed = false) {
    const long = targets(target);
    if (long.length > 0 && !confirmed) {
      confirmLong = { target, cards: long };
      return;
    }
    confirmLong = null;
    await save();
    if (!server || conflict) return;
    try {
      adopt(await api.generateScript(server.revision, target), false);
      say(
        "生成しています…ブラウザを閉じても続きます。初回はモデルを取得するため時間がかかります。",
        "busy",
      );
    } catch (error) {
      fail(error, "生成を始められませんでした。");
    }
  }

  async function stop() {
    try {
      adopt(await api.stopScript(), false);
      say("作成中のセリフが終わったら停止します。", "busy");
    } catch (error) {
      fail(error, "停止できませんでした。");
    }
  }

  async function play(from?: string) {
    await save();
    try {
      queue = (await api.playlist(from)).items;
    } catch (error) {
      fail(error, "再生できませんでした。");
      return;
    }
    say("");
    next();
  }

  function next() {
    const item = queue.shift();
    playingId = item?.id ?? "";
    if (item && player) {
      player.src = item.audio_url;
      player.volume = playback.volume;
      void player.play();
    }
  }

  function stopPlaying() {
    queue = [];
    playingId = "";
    player?.pause();
  }

  async function exportAs(kind: "wav" | "zip") {
    await save();
    say("保存する音声を用意しています…", "busy");
    try {
      const blob = await api.exportScript(kind);
      const link = document.createElement("a");
      link.href = URL.createObjectURL(blob);
      link.download = `まとめ音声.${kind}`;
      link.click();
      URL.revokeObjectURL(link.href);
      say("");
    } catch (error) {
      fail(error, "保存できませんでした。");
    }
  }

  const newId = () => crypto.randomUUID().replaceAll("-", "");

  function addCard() {
    void change((s) => {
      if (s.cards.length >= MAX_CARDS) {
        say(`セリフは${MAX_CARDS}枚までです。`, "error");
        return;
      }
      const source = s.options.inherit
        ? s.cards.find((c) => c.id === selected)
        : undefined;
      const card = makeCard(newId(), source);
      s.cards.push(card);
      selected = card.id;
    });
  }

  function act(
    index: number,
    action: "generate" | "duplicate" | "split" | "up" | "down" | "delete",
  ) {
    if (!script) return;
    const card = script.cards[index];
    if (action === "generate") {
      void generate(card.id);
      return;
    }
    if (action === "split") {
      let pieces: string[];
      try {
        pieces = splitText(card.text, readings, scale);
      } catch (error) {
        say((error as Error).message, "error");
        return;
      }
      if (script.cards.length + pieces.length - 1 > MAX_CARDS) {
        say(`セリフは${MAX_CARDS}枚までです。`, "error");
        return;
      }
      void change((s) => {
        s.cards[index].text = pieces[0];
        s.cards.splice(
          index + 1,
          0,
          ...pieces
            .slice(1)
            .map((text) => ({ ...makeCard(newId(), card), text })),
        );
      });
      say(
        pieces.length > 1
          ? `続きのセリフを${pieces.length - 1}枚追加しました。`
          : "分割は不要です。",
      );
      return;
    }
    const moved = change((s) => {
      if (action === "duplicate") {
        if (s.cards.length >= MAX_CARDS) return;
        const copy = { ...makeCard(newId(), card), text: card.text };
        s.cards.splice(index + 1, 0, copy);
        selected = copy.id;
      } else if (action === "delete") {
        s.cards.splice(index, 1);
        if (s.cards.length === 0) s.cards.push(makeCard(newId()));
        selected = s.cards[Math.min(index, s.cards.length - 1)].id;
      } else {
        s.cards = reorder(s.cards, index, index + (action === "up" ? -1 : 1));
      }
    });
    if (action === "up" || action === "down") {
      // Keep keyboard focus on the moved card's handle.
      void moved.then(() =>
        requestAnimationFrame(() =>
          document
            .querySelectorAll<HTMLElement>(".grip")
            [index + (action === "up" ? -1 : 1)]?.focus(),
        ),
      );
    }
  }

  function saveFile() {
    if (!script) return;
    const data = packScriptFile(snapshot(), voices);
    const link = document.createElement("a");
    link.href = URL.createObjectURL(
      new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }),
    );
    link.download = "台本.json";
    link.click();
    URL.revokeObjectURL(link.href);
  }

  async function loadFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !script) return;
    if (file.size > MAX_FILE_BYTES) {
      say("入力済データは7MBまでです。", "error");
      return;
    }
    const parsed = parseScriptFile(await file.text(), voices);
    if ("error" in parsed) {
      say(parsed.error, "error");
      return;
    }
    void change((s) => {
      s.cards = parsed.cards.map((card) => ({ ...card, id: newId() }));
      s.dictionary_enabled = parsed.dictionary_enabled;
      if (parsed.settings) s.settings = { ...s.settings, ...parsed.settings };
      selected = s.cards[0].id;
    });
    say(
      "入力済データを呼び出しました。" +
        (parsed.missing > 0
          ? ` ${parsed.missing}枚の話者を選び直してください。`
          : ""),
    );
  }

  function reset() {
    confirmReset = false;
    void change((s) => {
      s.cards = [makeCard(newId())];
      selected = s.cards[0].id;
    });
  }

  async function unload() {
    unloading = true;
    try {
      say((await api.unload()).message);
    } catch (error) {
      fail(error, "モデルを解放できませんでした。");
    } finally {
      unloading = false;
    }
  }

  function insertToken(token: string) {
    if (!script) return;
    script.options.pattern += token;
    edited();
  }

  $effect(() => {
    void loadInfo();
    void load();
    const refresh = () => {
      if (document.visibilityState === "visible" && edits === savedEdits) {
        void load();
      }
    };
    document.addEventListener("visibilitychange", refresh);
    return () => {
      document.removeEventListener("visibilitychange", refresh);
      clearTimeout(pollTimer);
      if (edits !== savedEdits) void save();
    };
  });
</script>

<div class="content script">
  {#if info.error}
    <p class="error-banner" role="alert">
      推論プロセスの設定を読み込めませんでした。{info.error}
    </p>
  {/if}
  {#if loadError}
    <div class="state-wrap">
      <p class="state error">{loadError}</p>
      <button class="btn" type="button" onclick={() => void load()}
        >再読み込み</button
      >
    </div>
  {:else if !script}
    <p class="state">
      <span class="spinner" aria-hidden="true"></span>読み込み中…
    </p>
  {:else}
    {#if conflict}
      <div class="error-banner conflict" role="alert">
        <span>別の画面で台本が更新されました。</span>
        <button class="btn" type="button" onclick={() => void load()}
          >最新の台本を読み込む</button
        >
      </div>
    {/if}
    {#if voices.length === 0}
      <p class="error-banner" role="alert">
        話者が登録されていません。<a href="/voices">声の登録</a
        >で話者を登録してください。
      </p>
    {/if}

    <div class="toolbar">
      <div class="row">
        {#if running}
          <button
            class="btn primary-wide"
            type="button"
            onclick={() => void stop()}
            disabled={job?.stopping}><Icon name="square" />停止</button
          >
          <progress max={job?.total} value={job?.done}></progress>
          <span class="progress-text">{job?.done} / {job?.total}</span>
        {:else}
          <button
            class="btn btn-primary primary-wide"
            type="button"
            onclick={() => void generate("changed")}
            >未生成・変更分を生成</button
          >
          <button class="btn" type="button" onclick={() => void generate("all")}
            >すべて生成</button
          >
        {/if}
      </div>
      <div class="row">
        {#if playingId}
          <button class="btn" type="button" onclick={stopPlaying}
            ><Icon name="square" />再生を止める</button
          >
        {:else}
          <button class="btn" type="button" onclick={() => void play()}
            ><Icon name="play" />連続再生</button
          >
          <button class="btn" type="button" onclick={() => void play(selected)}
            ><Icon name="play" />選択から再生</button
          >
        {/if}
        <button class="btn" type="button" onclick={() => void exportAs("wav")}
          ><Icon name="download" />つなげて保存</button
        >
        <button class="btn" type="button" onclick={() => void exportAs("zip")}
          ><Icon name="download" />個別にZIP</button
        >
        <span class="history">
          <button
            class="icon-btn"
            type="button"
            aria-label="元に戻す"
            title="元に戻す"
            disabled={undoStack.length === 0}
            onclick={() => void travel(true)}><Icon name="undo" /></button
          >
          <button
            class="icon-btn"
            type="button"
            aria-label="やり直す"
            title="やり直す"
            disabled={redoStack.length === 0}
            onclick={() => void travel(false)}><Icon name="redo" /></button
          >
        </span>
      </div>
      <p class="status" class:error={messageKind === "error"} role="status">
        {#if messageKind === "busy" && running}<span
            class="spinner"
            aria-hidden="true"
          ></span>{/if}{message}
      </p>
      <audio bind:this={player} onended={next} class="visually-hidden"></audio>
    </div>

    <div class="options-row">
      <label class="check">
        <input
          type="checkbox"
          bind:checked={script.dictionary_enabled}
          onchange={edited}
        />
        読み辞書を使う
      </label>
      <a href="/dictionary">読み辞書を編集</a>
      <label class="volume">
        再生音量
        <input
          type="range"
          min="0"
          max="100"
          value={Math.round(playback.volume * 100)}
          oninput={(event) =>
            saveVolume(Number(event.currentTarget.value) / 100)}
        />
        <output>{Math.round(playback.volume * 100)}%</output>
      </label>
    </div>

    <ol class="cards">
      {#each script.cards as card, index (card.id)}
        <ScriptCard
          bind:card={script.cards[index]}
          {index}
          count={script.cards.length}
          view={views.get(card.id)}
          {voices}
          selected={card.id === selected}
          playing={card.id === playingId}
          estimate={estimateSeconds(card.text, readings, scale)}
          long={isLong(card.text, readings, scale)}
          emojiGroups={info.value?.emoji_groups ?? []}
          busy={running}
          onedit={edited}
          onselect={() => (selected = card.id)}
          onaction={(action) => act(index, action)}
          ondragstart={() => (dragFrom = index)}
          ondrop={() => {
            if (dragFrom >= 0 && dragFrom !== index) {
              const from = dragFrom;
              void change((s) => (s.cards = reorder(s.cards, from, index)));
            }
            dragFrom = -1;
          }}
        />
      {/each}
    </ol>
    <button class="btn add" type="button" onclick={addCard}
      ><Icon name="plus" />セリフを追加</button
    >

    <div class="sections">
      <GenerationSettings
        bind:settings={script.settings}
        onchange={edited}
        onunload={() => void unload()}
        {unloading}
      />

      <details class="settings">
        <summary>オプション</summary>
        <div class="section-body" onchange={edited}>
          <label class="check">
            <input type="checkbox" bind:checked={script.options.inherit} />
            セリフを追加するとき、選択中のセリフの話者・話速・末尾の無音を引き継ぐ
          </label>
          <div class="field">
            <label for="pattern">保存するファイル名</label>
            <input
              id="pattern"
              class="input"
              bind:value={script.options.pattern}
              oninput={edited}
            />
            <div class="tokens">
              {#each TOKENS as token (token)}
                <button
                  class="btn btn-sm"
                  type="button"
                  onclick={() => insertToken(token)}>{token}</button
                >
              {/each}
            </div>
          </div>
          <label class="check">
            <input type="checkbox" bind:checked={script.options.txt} />
            ZIPに文章（TXT）も入れる
          </label>
        </div>
      </details>

      <details class="settings">
        <summary>入力済データ</summary>
        <div class="section-body">
          <p class="help">
            文章・話者の指定・各設定をファイルに保存し、呼び出せます。音声は含みません。
          </p>
          <div class="row">
            <button class="btn" type="button" onclick={saveFile}
              ><Icon name="download" />ファイルに保存</button
            >
            <label class="btn upload">
              <Icon name="upload" />ファイルから呼び出す
              <input
                class="visually-hidden"
                type="file"
                accept=".json,application/json"
                onchange={(event) => void loadFile(event)}
              />
            </label>
            <button
              class="btn"
              type="button"
              onclick={() => (confirmReset = true)}
              ><Icon name="trash" />文章をすべて消す</button
            >
          </div>
        </div>
      </details>
    </div>
  {/if}
</div>

{#if confirmLong}
  <Modal title="長いセリフの生成" onclose={() => (confirmLong = null)}>
    <p class="modal-text">
      セリフ {confirmLong.cards.slice(0, 20).join("・")}{confirmLong.cards
        .length > 20
        ? " ほか"
        : ""} は長いため、途中で切れたり不自然になったりする場合があります。分割（はさみのボタン）すると安定します。
    </p>
    <div class="buttons">
      <button
        class="btn"
        type="button"
        data-autofocus
        onclick={() => (confirmLong = null)}>キャンセル</button
      >
      <button
        class="btn btn-primary"
        type="button"
        onclick={() => void generate(confirmLong!.target, true)}
        >このまま生成</button
      >
    </div>
  </Modal>
{/if}

{#if confirmReset}
  <ConfirmModal
    title="文章をすべて消す"
    confirmLabel="消す"
    onconfirm={reset}
    oncancel={() => (confirmReset = false)}
  >
    すべてのセリフを消して、空のセリフ1枚にしますか？「元に戻す」で戻せます。生成した音声は生成履歴に残ります。
  </ConfirmModal>
{/if}

<style lang="sass">
  .script
    max-width: 960px

  .toolbar
    display: flex
    flex-direction: column
    gap: var(--sp-2)
    margin-bottom: var(--sp-3)
    padding: var(--sp-3)
    border: 1px solid var(--c-border)
    border-radius: var(--radius-lg)
    background: var(--c-surface-raised)

  .row
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2)

    .btn
      display: inline-flex
      align-items: center
      gap: var(--sp-1)

  .primary-wide
    min-height: 44px
    justify-content: center

  progress
    flex: 1
    min-width: 120px
    accent-color: var(--c-accent)

  .progress-text
    font-size: var(--fs-sm)

  .history
    display: inline-flex
    margin-left: auto

  .status
    margin: 0
    font-size: var(--fs-sm)
    color: var(--c-muted)

    &:empty
      display: none

    &.error
      color: var(--c-danger)

  .options-row
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2) var(--sp-4)
    margin-bottom: var(--sp-3)
    font-size: var(--fs-sm)

  .volume
    display: inline-flex
    align-items: center
    gap: var(--sp-2)

    input
      width: 120px
      accent-color: var(--c-accent)

  .cards
    display: flex
    flex-direction: column
    gap: var(--sp-2)
    margin: 0 0 var(--sp-3)
    padding: 0
    list-style: none

  .add
    display: inline-flex
    align-items: center
    gap: var(--sp-1)
    margin-bottom: var(--sp-4)

  .sections
    display: flex
    flex-direction: column
    gap: var(--sp-3)

  .settings
    border: 1px solid var(--c-border)
    border-radius: var(--radius-lg)
    background: var(--c-surface-raised)

    summary
      padding: var(--sp-3) var(--sp-4)
      font-size: var(--fs-md)
      font-weight: 500
      cursor: pointer

  .section-body
    display: flex
    flex-direction: column
    gap: var(--sp-3)
    padding: var(--sp-3) var(--sp-4) var(--sp-4)

  .tokens
    display: flex
    flex-wrap: wrap
    gap: var(--sp-1)

  .btn-sm
    padding: 4px 10px
    font-size: var(--fs-sm)

  .upload:focus-within
    outline: 2px solid var(--c-accent)
    outline-offset: 2px

  .conflict
    display: flex
    flex-wrap: wrap
    align-items: center
    justify-content: space-between
    gap: var(--sp-2)
    margin-bottom: var(--sp-3)

  .error-banner
    margin-top: 0

  .modal-text
    margin: 0 0 var(--sp-4)
    font-size: var(--fs-sm)

  .buttons
    display: flex
    justify-content: flex-end
    gap: var(--sp-2)
</style>
