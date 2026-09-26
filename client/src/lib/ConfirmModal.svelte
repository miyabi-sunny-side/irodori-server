<script lang="ts">
  import type { Snippet } from "svelte";

  import Modal from "./Modal.svelte";

  let {
    title,
    confirmLabel,
    onconfirm,
    oncancel,
    children,
  }: {
    title: string;
    confirmLabel: string;
    onconfirm: () => void;
    oncancel: () => void;
    children: Snippet;
  } = $props();
</script>

<Modal {title} onclose={oncancel}>
  <p class="message">{@render children()}</p>
  <div class="buttons">
    <button class="btn" type="button" data-autofocus onclick={oncancel}
      >キャンセル</button
    >
    <button class="btn btn-danger" type="button" onclick={onconfirm}
      >{confirmLabel}</button
    >
  </div>
</Modal>

<style lang="sass">
  .message
    margin: 0 0 var(--sp-4)
    font-size: var(--fs-sm)
    overflow-wrap: anywhere

  .buttons
    display: flex
    justify-content: flex-end
    gap: var(--sp-2)
</style>
