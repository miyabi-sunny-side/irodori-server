<script lang="ts">
  import {
    api,
    ApiError,
    downloadUrl,
    type FileError,
    type Generation,
  } from "../lib/api";
  import ConfirmModal from "../lib/ConfirmModal.svelte";
  import Icon from "../lib/Icon.svelte";
  import { info, loadInfo, playback } from "../lib/state.svelte";

  type ListState = "loading" | "empty" | "error" | "success";

  let generations = $state<Generation[]>([]);
  let fileErrors = $state<FileError[]>([]);
  let listState = $state<ListState>("loading");
  let pending = $state<Generation | null>(null);
  let deleteError = $state("");
  let returnFocus: HTMLElement | null = null;

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
</style>
