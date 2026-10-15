<script lang="ts">
  import { tick } from "svelte";

  import type { CardStatus, CardView, EmojiItem, Voice } from "./api";
  import { clampNumber, insertText } from "./create";
  import Icon from "./Icon.svelte";
  import type { ScriptCard } from "./script";
  import { modelLabel, playback } from "./state.svelte";

  let {
    card = $bindable(),
    index,
    count,
    view,
    voices,
    selected,
    playing,
    estimate,
    long,
    emojiGroups,
    busy,
    onedit,
    onselect,
    onaction,
    ondragstart,
    ondrop,
  }: {
    card: ScriptCard;
    index: number;
    count: number;
    view: CardView | undefined;
    voices: Voice[];
    selected: boolean;
    playing: boolean;
    estimate: number;
    long: boolean;
    emojiGroups: { title: string; items: EmojiItem[] }[];
    /** A generation run is going; single-card generation waits for it. */
    busy: boolean;
    onedit: () => void;
    onselect: () => void;
    onaction: (
      action: "generate" | "duplicate" | "split" | "up" | "down" | "delete",
    ) => void;
    ondragstart: () => void;
    ondrop: () => void;
  } = $props();

  const LABELS: Record<CardStatus, string> = {
    empty: "",
    no_voice: "話者を選択",
    new: "未作成",
    queued: "順番待ち",
    running: "作成中",
    done: "完了",
    changed: "変更あり",
    failed: "失敗",
  };

  let textArea = $state<HTMLTextAreaElement | undefined>();
  let emojiOpen = $state(false);
  let emojiTab = $state(0);
  let draggable = $state(false);
  let status = $derived(view?.status ?? "new");
  let number = $derived(index + 1);
  // A deleted voice stays selectable by its id until the card is changed.
  let missingVoice = $derived(
    card.voice_id !== null && !voices.some((v) => v.id === card.voice_id),
  );

  async function insertEmoji(emoji: string) {
    const area = textArea;
    const result = insertText(
      card.text,
      area?.selectionStart ?? null,
      area?.selectionEnd ?? null,
      emoji,
    );
    card.text = result.text;
    onedit();
    await tick();
    area?.focus({ preventScroll: true });
    area?.setSelectionRange(result.caret, result.caret);
  }

  function onGripKey(event: KeyboardEvent) {
    if (!event.altKey) return;
    if (event.key === "ArrowUp" && index > 0) {
      event.preventDefault();
      onaction("up");
    } else if (event.key === "ArrowDown" && index < count - 1) {
      event.preventDefault();
      onaction("down");
    }
  }

  function settle(key: "speed" | "silence", event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const [min, max] = key === "speed" ? [0.75, 1.5] : [0, 10];
    card[key] = clampNumber(input.valueAsNumber, min, max, card[key]);
    input.value = String(card[key]);
    onedit();
  }
</script>

<li
  class="card"
  class:selected
  class:playing
  data-status={status}
  {draggable}
  onfocusin={onselect}
  onpointerdown={onselect}
  ondragstart={(event) => {
    event.dataTransfer?.setData("text/plain", card.id);
    ondragstart();
  }}
  ondragend={() => (draggable = false)}
  ondragover={(event) => event.preventDefault()}
  ondrop={(event) => {
    event.preventDefault();
    draggable = false;
    ondrop();
  }}
  aria-label={`セリフ ${number}`}
>
  <div class="head">
    <button
      class="icon-btn grip"
      type="button"
      aria-label={`セリフ ${number}をつかんで移動（Alt＋↑／↓）`}
      title="つかんで移動（Alt＋↑／↓）"
      onpointerdown={() => (draggable = true)}
      onpointerup={() => (draggable = false)}
      onkeydown={onGripKey}><Icon name="grip" /></button
    >
    <span class="number">{number}</span>
    {#if LABELS[status]}
      <span class="badge" data-status={status}
        >{#if status === "running"}<span class="spinner" aria-hidden="true"
          ></span>{:else if status === "done"}<Icon
            name="check-check"
          />{/if}{LABELS[status]}</span
      >
    {/if}
    {#if card.text.trim()}
      <span class="estimate" class:long>約{estimate}秒</span>
    {/if}
    <span class="tools">
      <button
        class="icon-btn"
        type="button"
        aria-label={`セリフ ${number}を生成`}
        title="このセリフを生成"
        disabled={busy || !card.text.trim()}
        onclick={() => onaction("generate")}><Icon name="refresh-cw" /></button
      >
      <button
        class="icon-btn"
        type="button"
        aria-label={`セリフ ${number}に絵文字を挿入`}
        aria-expanded={emojiOpen}
        title="絵文字"
        onclick={() => (emojiOpen = !emojiOpen)}><Icon name="smile" /></button
      >
      {#if long}
        <button
          class="icon-btn"
          type="button"
          aria-label={`セリフ ${number}を分割`}
          title="長いセリフを分割"
          onclick={() => onaction("split")}><Icon name="scissors" /></button
        >
      {/if}
      <button
        class="icon-btn"
        type="button"
        aria-label={`セリフ ${number}を複製`}
        title="複製"
        onclick={() => onaction("duplicate")}><Icon name="copy" /></button
      >
      <button
        class="icon-btn"
        type="button"
        aria-label={`セリフ ${number}を削除`}
        title="削除"
        onclick={() => onaction("delete")}><Icon name="trash" /></button
      >
    </span>
  </div>

  <div class="fields">
    <select
      class="input voice"
      aria-label={`セリフ ${number}の話者`}
      value={card.voice_id === null ? "" : String(card.voice_id)}
      onchange={(event) => {
        const value = event.currentTarget.value;
        card.voice_id = value ? Number(value) : null;
        onedit();
      }}
    >
      <option value="">話者を選択</option>
      {#if missingVoice}
        <option value={String(card.voice_id)}>（削除された話者）</option>
      {/if}
      {#each voices as voice (voice.id)}
        <option value={String(voice.id)}
          >{voice.name}（{modelLabel(voice.model)}）</option
        >
      {/each}
    </select>
    <label class="small">
      話速
      <input
        class="input number"
        type="number"
        min="0.75"
        max="1.5"
        step="0.05"
        value={card.speed}
        onchange={(event) => settle("speed", event)}
      />
    </label>
    <label class="small">
      末尾の無音
      <input
        class="input number"
        type="number"
        min="0"
        max="10"
        step="0.5"
        value={card.silence}
        onchange={(event) => settle("silence", event)}
      />
      秒
    </label>
  </div>

  <textarea
    class="input text"
    rows="2"
    aria-label={`セリフ ${number}の文章`}
    bind:this={textArea}
    bind:value={card.text}
    oninput={onedit}></textarea>

  {#if emojiOpen && emojiGroups.length > 0}
    <div class="emoji-tabs" role="tablist" aria-label="絵文字の分類">
      {#each emojiGroups as group, tab (group.title)}
        <button
          type="button"
          role="tab"
          aria-selected={emojiTab === tab}
          onclick={() => (emojiTab = tab)}>{group.title}</button
        >
      {/each}
    </div>
    <div class="emoji-panel" role="tabpanel">
      {#each emojiGroups[emojiTab]?.items ?? [] as item (item.label)}
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
  {/if}

  {#if view?.audio_url}
    <div class="audio">
      <audio
        controls
        preload="none"
        src={view.audio_url}
        bind:volume={playback.volume}
        aria-label={`セリフ ${number}の音声`}
      ></audio>
      <a
        class="icon-btn"
        href={view.audio_url}
        download={view.download_name}
        aria-label={`セリフ ${number}のWAVをダウンロード`}
        title="WAVをダウンロード"><Icon name="download" /></a
      >
    </div>
  {/if}
  {#if view?.error && (status === "failed" || status === "changed" || status === "done")}
    <p class="help error">{view.error}</p>
  {/if}
</li>

<style lang="sass">
  .card
    display: flex
    flex-direction: column
    gap: var(--sp-2)
    padding: 10px
    border: 1px solid var(--c-border)
    border-radius: var(--radius-md)
    background: var(--c-surface-raised)

    &.selected
      border-color: var(--c-accent)

    &.playing
      box-shadow: inset 3px 0 0 var(--c-accent)

  .head
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-1) var(--sp-2)
    margin: -4px -4px 0

  .grip
    cursor: grab
    color: var(--c-muted)

  .number
    font-size: var(--fs-sm)
    font-weight: 600

  .badge
    display: inline-flex
    align-items: center
    gap: var(--sp-1)
    padding: 0 var(--sp-2)
    border: 1px solid var(--c-border)
    border-radius: var(--radius-sm)
    font-size: var(--fs-xs)
    line-height: 22px
    color: var(--c-muted)

    .spinner
      width: 0.8rem
      height: 0.8rem
      margin: 0

    &[data-status="done"]
      color: var(--c-accent)

    &[data-status="failed"], &[data-status="no_voice"]
      border-color: var(--c-danger)
      color: var(--c-danger)

    &[data-status="changed"], &[data-status="running"]
      color: var(--c-on-surface)

  .estimate
    font-size: var(--fs-xs)
    color: var(--c-muted)

    &.long
      color: var(--c-danger)

  .tools
    display: flex
    flex-wrap: wrap
    margin-left: auto

  .fields
    display: flex
    flex-wrap: wrap
    align-items: center
    gap: var(--sp-2) var(--sp-3)

  .voice
    flex: 1 1 200px
    width: auto
    min-width: 0

  .small
    display: inline-flex
    align-items: center
    gap: var(--sp-1)
    font-size: var(--fs-sm)
    white-space: nowrap

  .number.input
    width: 76px
    padding: 6px

  .text
    min-height: 3.5em

  .audio
    display: flex
    align-items: center
    gap: var(--sp-2)

    audio
      flex: 1
      min-width: 0
      height: 40px

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
</style>
