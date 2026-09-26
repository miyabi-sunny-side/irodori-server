<script lang="ts">
  import { clampNumber } from "./create";

  let {
    label,
    help = "",
    min,
    max,
    step = 1,
    value = $bindable(),
    disabled = false,
  }: {
    label: string;
    help?: string;
    min: number;
    max: number;
    step?: number;
    value: number;
    disabled?: boolean;
  } = $props();

  const id = $props.id();

  // The number box accepts typing freely and settles into the range on change.
  function settle(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    value = clampNumber(input.valueAsNumber, min, max, value);
    input.value = String(value);
  }
</script>

<div class="field slider" class:disabled>
  <label for={id}>{label}</label>
  <div class="slider-row">
    <input
      type="range"
      {min}
      {max}
      {step}
      {disabled}
      bind:value
      aria-label={label}
      aria-describedby={help ? `${id}-help` : undefined}
    />
    <input
      {id}
      class="input number"
      type="number"
      {min}
      {max}
      {step}
      {disabled}
      {value}
      onchange={settle}
      aria-describedby={help ? `${id}-help` : undefined}
    />
  </div>
  {#if help}
    <p id={`${id}-help`} class="help">{help}</p>
  {/if}
</div>

<style lang="sass">
  .slider-row
    display: flex
    align-items: center
    gap: var(--sp-3)

    input[type="range"]
      flex: 1
      min-width: 0
      accent-color: var(--c-accent)

  .number
    width: 88px
    flex: none

  .disabled
    opacity: 0.5
</style>
