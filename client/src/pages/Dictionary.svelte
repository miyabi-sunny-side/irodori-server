<script lang="ts">
  import { api, ApiError, type DictionaryEntry } from "../lib/api";
  import ConfirmModal from "../lib/ConfirmModal.svelte";
  import Icon from "../lib/Icon.svelte";

  type ListState = "loading" | "empty" | "error" | "success";

  const EXAMPLES = [
    ["Irodori", "いろどり"],
    ["TTS", "てぃーてぃーえす"],
    ["日本橋", "にほんばし"],
  ] as const;

  let entries = $state<DictionaryEntry[]>([]);
  let listState = $state<ListState>("loading");
  let word = $state("");
  let reading = $state("");
  let message = $state("");
  let failed = $state(false);
  let saving = $state(false);
  let pending = $state<DictionaryEntry | null>(null);
  let returnFocus: HTMLElement | null = null;
  let wordInput = $state<HTMLInputElement | undefined>();

  function show(result: { message: string; entries: DictionaryEntry[] }) {
    entries = result.entries;
    listState = entries.length === 0 ? "empty" : "success";
    message = result.message;
    failed = false;
  }

  function fail(error: unknown, fallback: string) {
    message = error instanceof ApiError ? error.message : fallback;
    failed = true;
  }

  async function load() {
    listState = "loading";
    try {
      entries = (await api.dictionary()).entries;
      listState = entries.length === 0 ? "empty" : "success";
    } catch {
      listState = "error";
    }
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    saving = true;
    try {
      show(await api.saveEntry(word, reading));
    } catch (error) {
      fail(error, "登録できませんでした。");
    } finally {
      saving = false;
    }
  }

  function edit(entry: DictionaryEntry) {
    word = entry.word;
    reading = entry.reading;
    wordInput?.focus();
  }

  function askDelete(entry: DictionaryEntry, event: MouseEvent) {
    returnFocus = event.currentTarget as HTMLElement;
    pending = entry;
  }

  async function confirmDelete() {
    const entry = pending;
    pending = null;
    if (!entry) {
      return;
    }
    try {
      show(await api.deleteEntry(entry.word));
    } catch (error) {
      fail(error, "削除できませんでした。");
    }
    wordInput?.focus();
  }

  function cancelDelete() {
    pending = null;
    returnFocus?.focus();
  }

  $effect(() => {
    void load();
  });
</script>

<div class="content">
  <p class="lead">
    名前や略語など、読み間違えやすい表記と読み方を登録します。登録内容は再起動しても残ります。
  </p>
  <form class="entry-form" onsubmit={save}>
    <div class="inputs">
      <div class="field">
        <label for="word">表記</label>
        <input
          id="word"
          class="input"
          placeholder="例：Irodori"
          bind:value={word}
          bind:this={wordInput}
          aria-describedby="word-help"
        />
        <p id="word-help" class="help">
          文章の中で置き換えたい文字です。大文字・小文字も区別します。
        </p>
      </div>
      <div class="field">
        <label for="reading">読み方</label>
        <input
          id="reading"
          class="input"
          placeholder="例：いろどり"
          bind:value={reading}
          aria-describedby="reading-help"
        />
        <p id="reading-help" class="help">
          ひらがな・カタカナなどで読み方を書きます。アクセントの指定機能ではありません。
        </p>
      </div>
    </div>
    <div class="examples">
      <span class="help">入力例（押すと入力欄に入ります）</span>
      {#each EXAMPLES as [exampleWord, exampleReading] (exampleWord)}
        <button
          class="btn example"
          type="button"
          onclick={() => {
            word = exampleWord;
            reading = exampleReading;
          }}>{exampleWord} → {exampleReading}</button
        >
      {/each}
    </div>
    <div class="actions">
      <button class="btn btn-primary" type="submit" disabled={saving}
        >登録・更新</button
      >
      {#if message}
        <p class="result" class:error={failed} role="status">{message}</p>
      {/if}
    </div>
  </form>

  <h2>登録済みの読み方</h2>
  <div data-state={listState}>
    {#if listState === "loading"}
      <p class="state">
        <span class="spinner" aria-hidden="true"></span>読み込み中…
      </p>
    {:else if listState === "empty"}
      <p class="state">まだ登録はありません。</p>
    {:else if listState === "error"}
      <div class="state-wrap">
        <p class="state error">読み辞書を読み込めませんでした。</p>
        <button class="btn" type="button" onclick={() => void load()}>
          再読み込み
        </button>
      </div>
    {:else}
      <table class="entries">
        <thead>
          <tr
            ><th>表記</th><th>読み方</th><th
              ><span class="visually-hidden">操作</span></th
            ></tr
          >
        </thead>
        <tbody>
          {#each entries as entry (entry.word)}
            <tr>
              <td>{entry.word}</td>
              <td>{entry.reading}</td>
              <td class="row-actions">
                <button
                  class="icon-btn"
                  type="button"
                  aria-label={`${entry.word}を編集`}
                  onclick={() => edit(entry)}><Icon name="pencil" /></button
                >
                <button
                  class="icon-btn"
                  type="button"
                  aria-label={`${entry.word}を削除`}
                  onclick={(event) => askDelete(entry, event)}
                  ><Icon name="trash" /></button
                >
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
  <p class="help rules">
    置き換えのルール：文章中の一致する部分を置換します。長い表記を優先し、置換後の文字は再置換しません。短い表記は別の単語にも一致するため、音声作成の「読み辞書を反映した文章を確認」で確かめてください。声の説明や元の入力文は変更しません。
  </p>
</div>

{#if pending}
  <ConfirmModal
    title="読み方の削除"
    confirmLabel="削除する"
    onconfirm={() => void confirmDelete()}
    oncancel={cancelDelete}
  >
    「{pending.word}」の読み方（{pending.reading}）を削除しますか？
  </ConfirmModal>
{/if}

<style lang="sass">
  .lead
    margin: 0 0 var(--sp-4)
    font-size: var(--fs-sm)
    color: var(--c-muted)

  .entry-form
    display: flex
    flex-direction: column
    gap: var(--sp-3)

  .inputs
    display: grid
    gap: var(--sp-3)

  @media (min-width: 768px)
    .inputs
      grid-template-columns: 1fr 1fr

  .examples
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2)

  .example
    padding: var(--sp-1) var(--sp-2)
    font-size: var(--fs-sm)
    font-weight: 400

  .actions
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-3)

  .result
    margin: 0
    font-size: var(--fs-sm)
    color: var(--c-muted)

    &.error
      color: var(--c-danger)

  h2
    margin: var(--sp-5) 0 var(--sp-2)
    font-size: var(--fs-xl)
    font-weight: 600
    line-height: 1.3

  .entries
    width: 100%
    border-collapse: collapse
    font-size: var(--fs-sm)

    th, td
      padding: var(--sp-1) var(--sp-2)
      border-bottom: 1px solid var(--c-border)
      text-align: left
      overflow-wrap: anywhere

    th
      color: var(--c-muted)
      font-weight: 500

  .row-actions
    width: 80px
    white-space: nowrap

    button
      display: inline-flex

  .rules
    margin-top: var(--sp-4)
</style>
