<script lang="ts">
  import { api, ApiError, type Reference } from "../lib/api";
  import ConfirmModal from "../lib/ConfirmModal.svelte";
  import Icon from "../lib/Icon.svelte";
  import { groupByCharacter } from "../lib/references";
  import { playback } from "../lib/state.svelte";

  type ListState = "loading" | "empty" | "error" | "success";

  let references = $state<Reference[]>([]);
  let listState = $state<ListState>("loading");
  let pending = $state<Reference | null>(null);
  let deleteError = $state("");
  let returnFocus: HTMLElement | null = null;

  let groups = $derived(groupByCharacter(references));

  const formatDate = (value: string | undefined) =>
    value
      ? new Date(value).toLocaleString("ja-JP", {
          dateStyle: "short",
          timeStyle: "short",
        })
      : "";

  async function load() {
    try {
      references = (await api.references()).references;
      listState = references.length === 0 ? "empty" : "success";
    } catch {
      listState = "error";
    }
  }

  function askDelete(reference: Reference, event: MouseEvent) {
    returnFocus = event.currentTarget as HTMLElement;
    deleteError = "";
    pending = reference;
  }

  function cancelDelete() {
    pending = null;
    returnFocus?.focus();
  }

  async function confirmDelete() {
    const reference = pending;
    pending = null;
    if (!reference) {
      return;
    }
    try {
      await api.deleteReference(reference.id);
      await load();
    } catch (error) {
      deleteError =
        error instanceof ApiError ? error.message : "削除できませんでした。";
      returnFocus?.focus();
    }
  }

  $effect(() => {
    void load();
  });
</script>

<div class="content" data-state={listState}>
  {#if deleteError}
    <p class="error-banner" role="alert">{deleteError}</p>
  {/if}

  {#if listState === "loading"}
    <p class="state">
      <span class="spinner" aria-hidden="true"></span>読み込み中…
    </p>
  {:else if listState === "empty"}
    <div class="state-wrap">
      <p class="state">
        まだお手本はありません。生成履歴の「お手本に保存」で追加できます。
      </p>
      <a href="/history">生成履歴へ</a>
    </div>
  {:else if listState === "error"}
    <div class="state-wrap">
      <p class="state error">お手本を読み込めませんでした。</p>
      <button class="btn" type="button" onclick={() => void load()}>
        再読み込み
      </button>
    </div>
  {:else}
    <p class="help intro">
      音声作成の「お手本の声に似せる」「お手本の声＋話し方を指定」で選べます。
    </p>
    {#each groups as group, index (group.character)}
      <section class="group" aria-labelledby={`character-${index}`}>
        <h2 id={`character-${index}`}>
          {group.character}<span class="count">{group.items.length}件</span>
        </h2>
        <ul class="cards">
          {#each group.items as reference (reference.id)}
            <li class="card">
              <p class="meta">
                {reference.name}
                {#if reference.source_generation_id}・元の生成 #{reference.source_generation_id}{/if}
                ・<time datetime={reference.created_at}
                  >{formatDate(reference.created_at)}</time
                >
              </p>
              <div class="actions">
                <audio
                  controls
                  preload="none"
                  src={reference.audio_url}
                  bind:volume={playback.volume}
                  aria-label={`${group.character} ${reference.name}の音声`}
                ></audio>
                <button
                  class="icon-btn"
                  type="button"
                  aria-label={`${group.character} ${reference.name}を削除`}
                  onclick={(event) => askDelete(reference, event)}
                  ><Icon name="trash" /></button
                >
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</div>

{#if pending}
  <ConfirmModal
    title="お手本の削除"
    confirmLabel="削除する"
    onconfirm={() => void confirmDelete()}
    oncancel={cancelDelete}
  >
    「{pending.character}」のお手本「{pending.name}」を削除しますか？お手本のWAVファイルを削除し、元に戻せません。生成履歴の音声は残ります。
  </ConfirmModal>
{/if}

<style lang="sass">
  .intro
    margin: 0 0 var(--sp-3)

  .group + .group
    margin-top: var(--sp-5)

  h2
    margin: 0 0 var(--sp-2)
    font-size: var(--fs-xl)
    overflow-wrap: anywhere

  .count
    margin-left: var(--sp-2)
    white-space: nowrap
    font-size: var(--fs-xs)
    font-weight: 400
    color: var(--c-muted)

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

  .error-banner
    margin-top: 0
</style>
