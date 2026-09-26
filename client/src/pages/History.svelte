<script lang="ts">
  import { tick } from "svelte";

  import {
    api,
    ApiError,
    downloadUrl,
    type FileError,
    type Generation,
  } from "../lib/api";
  import ConfirmModal from "../lib/ConfirmModal.svelte";
  import Icon from "../lib/Icon.svelte";
  import Modal from "../lib/Modal.svelte";
  import {
    CHARACTER_MAX,
    characterName,
    unsavedCount,
  } from "../lib/references";
  import { info, loadInfo, playback } from "../lib/state.svelte";

  type ListState = "loading" | "empty" | "error" | "success";

  let generations = $state<Generation[]>([]);
  let fileErrors = $state<FileError[]>([]);
  let listState = $state<ListState>("loading");
  let pending = $state<Generation | null>(null);
  let deleteError = $state("");
  let returnFocus: HTMLElement | null = null;
  let characters = $state<string[]>([]);
  let saving = $state<Generation | null>(null);
  let character = $state("");
  let saveError = $state("");
  let saveBusy = $state(false);
  let cleanupOpen = $state(false);
  let message = $state("");

  let unsaved = $derived(unsavedCount(generations));

  const label = (
    choices: { id: string; label: string }[] | undefined,
    id: string,
  ) => choices?.find((choice) => choice.id === id)?.label ?? id;

  const formatDate = (value: string) =>
    new Date(value).toLocaleString("ja-JP", {
      dateStyle: "short",
      timeStyle: "short",
    });

  async function load() {
    try {
      const result = await api.generations();
      generations = result.generations;
      fileErrors = result.file_errors;
      listState = generations.length === 0 ? "empty" : "success";
    } catch {
      listState = "error";
    }
  }

  function askDelete(generation: Generation, event: MouseEvent) {
    returnFocus = event.currentTarget as HTMLElement;
    deleteError = "";
    pending = generation;
  }

  async function confirmDelete() {
    const generation = pending;
    pending = null;
    if (!generation) {
      return;
    }
    try {
      await api.deleteGeneration(generation.id);
      await load();
    } catch (error) {
      deleteError =
        error instanceof ApiError ? error.message : "削除できませんでした。";
      returnFocus?.focus();
    }
  }

  function cancelDelete() {
    pending = null;
    returnFocus?.focus();
  }

  async function loadCharacters() {
    try {
      const names = (await api.references()).references.map(
        (reference) => reference.character ?? "",
      );
      characters = [...new Set(names.filter(Boolean))];
    } catch {
      characters = [];
    }
  }

  function askSave(generation: Generation, event: MouseEvent) {
    returnFocus = event.currentTarget as HTMLElement;
    saveError = "";
    saving = generation;
    void loadCharacters();
  }

  function cancelSave() {
    saving = null;
    returnFocus?.focus();
  }

  async function confirmSave(event: SubmitEvent) {
    event.preventDefault();
    const generation = saving;
    const checked = characterName(character);
    if (!generation) {
      return;
    }
    if ("error" in checked) {
      saveError = checked.error;
      return;
    }
    saveBusy = true;
    try {
      await api.saveReference(generation.id, checked.value);
      generation.saved = true;
      saving = null;
      message = `「${checked.value}」のお手本に保存しました。`;
      await tick();
      document.getElementById(`saved-${generation.id}`)?.focus();
    } catch (error) {
      saveError =
        error instanceof ApiError ? error.message : "保存できませんでした。";
    } finally {
      saveBusy = false;
    }
  }

  function askCleanup(event: MouseEvent) {
    returnFocus = event.currentTarget as HTMLElement;
    deleteError = "";
    cleanupOpen = true;
  }

  function cancelCleanup() {
    cleanupOpen = false;
    returnFocus?.focus();
  }

  async function confirmCleanup() {
    cleanupOpen = false;
    try {
      const { deleted } = await api.cleanupGenerations();
      message = `${deleted}件の音声を削除しました。`;
      await load();
    } catch (error) {
      deleteError =
        error instanceof ApiError ? error.message : "削除できませんでした。";
    }
    returnFocus?.focus();
  }

  $effect(() => {
    void loadInfo();
    void load();
  });
</script>

<div class="content" data-state={listState}>
  {#if fileErrors.length > 0}
    <div class="error-banner file-errors" role="alert">
      <p>削除できなかったファイルがあります。記録は削除済みです。</p>
      <ul>
        {#each fileErrors as fileError (fileError.path + fileError.created_at)}
          <li><code>{fileError.path}</code>：{fileError.error}</li>
        {/each}
      </ul>
    </div>
  {/if}
  {#if deleteError}
    <p class="error-banner" role="alert">{deleteError}</p>
  {/if}
  {#if listState === "success"}
    <div class="toolbar">
      <button
        class="btn"
        type="button"
        disabled={unsaved === 0}
        onclick={askCleanup}
        ><Icon
          name="trash"
        />保存していない音声を一括削除（{unsaved}件）</button
      >
      <p class="help">
        お手本に保存した音声は残ります。<a href="/references">お手本の一覧</a>
      </p>
    </div>
  {/if}
  <p class="message" role="status">{message}</p>

  {#if listState === "loading"}
    <p class="state">
      <span class="spinner" aria-hidden="true"></span>読み込み中…
    </p>
  {:else if listState === "empty"}
    <div class="state-wrap">
      <p class="state">まだ生成した音声はありません。</p>
      <a href="/">音声作成へ</a>
    </div>
  {:else if listState === "error"}
    <div class="state-wrap">
      <p class="state error">生成履歴を読み込めませんでした。</p>
      <button class="btn" type="button" onclick={() => void load()}>
        再読み込み
      </button>
    </div>
  {:else}
    <ul class="cards">
      {#each generations as generation (generation.id)}
        <li class="card">
          <p class="text">{generation.text}</p>
          <p class="meta">
            <time datetime={generation.created_at}
              >{formatDate(generation.created_at)}</time
            >
            ・{label(info.value?.modes, generation.mode)}
            ・{label(info.value?.models, generation.model)}
            ・話速 {generation.speed.toFixed(2)}倍
            {#if generation.candidate > 1}・候補 {generation.candidate}{/if}
          </p>
          <div class="save-row">
            {#if generation.saved}
              <span class="saved" id={`saved-${generation.id}`} tabindex="-1"
                ><Icon name="check-check" />お手本に保存済み</span
              >
            {:else}
              <button
                class="btn btn-sm"
                type="button"
                aria-label={`「${generation.text}」をお手本に保存`}
                onclick={(event) => askSave(generation, event)}
                ><Icon name="book" />お手本に保存</button
              >
            {/if}
          </div>
          <div class="actions">
            <audio
              controls
              preload="none"
              src={generation.audio_url}
              bind:volume={playback.volume}
              aria-label={`${generation.text}の音声`}
            ></audio>
            <a
              class="icon-btn"
              href={downloadUrl(generation)}
              download
              aria-label={`${formatDate(generation.created_at)}の音声をダウンロード`}
              ><Icon name="download" /></a
            >
            <button
              class="icon-btn"
              type="button"
              aria-label={`${formatDate(generation.created_at)}の音声を削除`}
              onclick={(event) => askDelete(generation, event)}
              ><Icon name="trash" /></button
            >
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if saving}
  <Modal title="お手本に保存" onclose={cancelSave}>
    <form class="save-form" onsubmit={(event) => void confirmSave(event)}>
      <p class="save-text">「{saving.text}」</p>
      <div class="field">
        <label for="character">キャラクター名</label>
        <input
          id="character"
          class="input"
          list="character-names"
          maxlength={CHARACTER_MAX}
          autocomplete="off"
          data-autofocus
          aria-describedby="character-help"
          aria-invalid={saveError ? "true" : undefined}
          bind:value={character}
        />
        <datalist id="character-names">
          {#each characters as name (name)}
            <option value={name}></option>
          {/each}
        </datalist>
        <p class="help" id="character-help">
          1行・{CHARACTER_MAX}文字まで。同じ名前のお手本はまとめて表示します。
        </p>
        {#if saveError}
          <p class="help error" role="alert">{saveError}</p>
        {/if}
      </div>
      <div class="buttons">
        <button class="btn" type="button" onclick={cancelSave}
          >キャンセル</button
        >
        <button class="btn btn-primary" type="submit" disabled={saveBusy}
          >保存する</button
        >
      </div>
    </form>
  </Modal>
{/if}

{#if cleanupOpen}
  <ConfirmModal
    title="保存していない音声の一括削除"
    confirmLabel="削除する"
    onconfirm={() => void confirmCleanup()}
    oncancel={cancelCleanup}
  >
    お手本に保存していない{unsaved}件の音声を削除しますか？記録とWAVファイルを削除し、元に戻せません。お手本に保存した音声は残ります。
  </ConfirmModal>
{/if}

{#if pending}
  <ConfirmModal
    title="音声の削除"
    confirmLabel="削除する"
    onconfirm={() => void confirmDelete()}
    oncancel={cancelDelete}
  >
    「{pending.text}」の音声を削除しますか？記録とWAVファイルを削除し、元に戻せません。
  </ConfirmModal>
{/if}

<style lang="sass">
  .cards
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

  .text
    display: -webkit-box
    margin: 0
    overflow: hidden
    -webkit-line-clamp: 2
    -webkit-box-orient: vertical
    overflow-wrap: anywhere

  .meta
    margin: 0
    font-size: var(--fs-xs)
    color: var(--c-muted)

  .actions
    display: flex
    align-items: center
    gap: var(--sp-2)

    audio
      flex: 1
      min-width: 0
      height: 40px

  .file-errors
    margin-bottom: var(--sp-3)

    p
      margin: 0 0 var(--sp-1)

    ul
      margin: 0
      padding-left: var(--sp-4)
      overflow-wrap: anywhere

  .error-banner
    margin-top: 0

  .toolbar
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2) var(--sp-3)
    margin-bottom: var(--sp-2)

    .help
      margin: 0

  .message
    margin: 0 0 var(--sp-2)
    font-size: var(--fs-sm)

    &:empty
      display: none

  .save-row
    display: flex

  .toolbar .btn, .btn-sm
    display: inline-flex
    align-items: center
    gap: var(--sp-1)

  .btn-sm
    padding: 4px 10px
    font-size: var(--fs-sm)

  .saved
    display: inline-flex
    align-items: center
    gap: var(--sp-1)
    font-size: var(--fs-sm)
    color: var(--c-accent)

  .save-text
    display: -webkit-box
    margin: 0 0 var(--sp-3)
    overflow: hidden
    -webkit-line-clamp: 2
    -webkit-box-orient: vertical
    font-size: var(--fs-sm)
    overflow-wrap: anywhere

  .buttons
    display: flex
    justify-content: flex-end
    gap: var(--sp-2)
    margin-top: var(--sp-4)
</style>
