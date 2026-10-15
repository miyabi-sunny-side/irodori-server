<script lang="ts">
  import type { GenerateRequest } from "./api";
  import { parseSteps } from "./create";
  import Slider from "./Slider.svelte";
  import { info } from "./state.svelte";

  // Settings shared by every card of the script. Blank devices and
  // precisions follow the engine (bf16 on a GPU that offers it).
  let {
    settings = $bindable(),
    onchange,
    onunload,
    unloading = false,
  }: {
    settings: GenerateRequest;
    onchange: () => void;
    onunload: () => void;
    unloading?: boolean;
  } = $props();

  const RAW_FIELDS = [
    ["cfg_scale_raw", "反映度の一括上書き"],
    ["speaker_kv_scale_raw", "お手本の内部補正値"],
    ["max_text_len_raw", "文章の処理上限（トークン数）"],
    ["max_caption_len_raw", "説明の処理上限（トークン数）"],
    ["truncation_factor_raw", "ノイズの打ち切り係数"],
    ["rescale_k_raw", "再スケーリング係数"],
    ["rescale_sigma_raw", "再スケーリングの強さ"],
    ["lora_adapter_raw", "LoRAフォルダの場所"],
  ] as const;

  const precisionsFor = (device: string) =>
    info.value?.precisions[device || (info.value?.devices[0] ?? "")] ?? [];

  function changeDevice(which: "model" | "codec") {
    if (which === "model") settings.model_precision = "";
    else settings.codec_precision = "";
    onchange();
  }
</script>

<details class="settings">
  <summary>生成設定</summary>
  <div class="settings-grid" {onchange}>
    <div class="field">
      <label for="steps">生成の計算回数</label>
      <input
        id="steps"
        class="input"
        inputmode="numeric"
        placeholder="モデルの標準"
        value={settings.num_steps ?? ""}
        onchange={(event) => {
          settings.num_steps = parseSteps(event.currentTarget.value);
          event.currentTarget.value = String(settings.num_steps ?? "");
        }}
        aria-describedby="steps-help"
      />
      <p id="steps-help" class="help">空欄で標準40・MF4。1～120。</p>
    </div>
    <Slider
      label="生成する長さの倍率"
      help="1が標準。大きいほど長めに生成します。"
      min={0.5}
      max={1.5}
      step={0.01}
      bind:value={settings.duration_scale}
    />
    <div class="field">
      <label for="seed">再現用の番号（シード）</label>
      <input
        id="seed"
        class="input"
        bind:value={settings.seed}
        aria-describedby="seed-help"
      />
      <p id="seed-help" class="help">空欄なら毎回ランダムです。</p>
    </div>
    <div class="field">
      <label for="seconds">生成する秒数（任意）</label>
      <input id="seconds" class="input" bind:value={settings.seconds} />
    </div>
    <Slider
      label="文章への忠実さ"
      help="MFの話者には適用されません。"
      min={0}
      max={10}
      step={0.1}
      bind:value={settings.cfg_scale_text}
    />
    <Slider
      label="声の説明の反映度"
      help="MFの話者には適用されません。"
      min={0}
      max={10}
      step={0.1}
      bind:value={settings.cfg_scale_caption}
    />
    <Slider
      label="お手本の声の反映度"
      help="MFの話者には適用されません。"
      min={0}
      max={10}
      step={0.1}
      bind:value={settings.cfg_scale_speaker}
    />
  </div>
</details>

<details class="settings">
  <summary>詳細設定（通常は変更不要）</summary>
  <div class="settings-grid" {onchange}>
    {#each [["model", "音声生成"], ["codec", "音声変換"]] as const as [which, label] (which)}
      <div class="field">
        <label for={`${which}-device`}>{label}に使う機器</label>
        {#if which === "model"}
          <select
            id="model-device"
            class="input"
            bind:value={settings.model_device}
            onchange={(event) => {
              event.stopPropagation();
              changeDevice("model");
            }}
          >
            <option value="">自動</option>
            {#each info.value?.devices ?? [] as device (device)}
              <option value={device}>{device}</option>
            {/each}
          </select>
        {:else}
          <select
            id="codec-device"
            class="input"
            bind:value={settings.codec_device}
            onchange={(event) => {
              event.stopPropagation();
              changeDevice("codec");
            }}
          >
            <option value="">自動</option>
            {#each info.value?.devices ?? [] as device (device)}
              <option value={device}>{device}</option>
            {/each}
          </select>
        {/if}
      </div>
      <div class="field">
        <label for={`${which}-precision`}>{label}の計算精度</label>
        {#if which === "model"}
          <select
            id="model-precision"
            class="input"
            bind:value={settings.model_precision}
          >
            <option value="">自動</option>
            {#each precisionsFor(settings.model_device) as precision (precision)}
              <option value={precision}>{precision}</option>
            {/each}
          </select>
        {:else}
          <select
            id="codec-precision"
            class="input"
            bind:value={settings.codec_precision}
          >
            <option value="">自動</option>
            {#each precisionsFor(settings.codec_device) as precision (precision)}
              <option value={precision}>{precision}</option>
            {/each}
          </select>
        {/if}
      </div>
    {/each}
    <p class="help wide">
      自動はGPUがあればGPU（bf16）を使います。Large（INT8・INT4）はGPU専用です。
    </p>
    <div class="field">
      <label for="schedule">計算の進め方</label>
      <select id="schedule" class="input" bind:value={settings.t_schedule_mode}>
        <option value="linear">標準</option>
        <option value="sway">Sway（実験的）</option>
      </select>
    </div>
    <Slider
      label="Swayの調整値"
      help="Swayを選んだ場合だけ使います。MFの話者には適用されません。"
      min={-1}
      max={1.5}
      step={0.1}
      disabled={settings.t_schedule_mode !== "sway"}
      bind:value={settings.sway_coeff}
    />
    <div class="field">
      <label for="guidance">条件を反映する方式</label>
      <select
        id="guidance"
        class="input"
        bind:value={settings.cfg_guidance_mode}
      >
        <option value="independent">個別</option>
        <option value="joint">一括</option>
        <option value="alternating">交互</option>
      </select>
    </div>
    <div class="field">
      <label class="check">
        <input type="checkbox" bind:checked={settings.context_kv_cache} />
        共通の計算結果を再利用する
      </label>
    </div>
    {#each RAW_FIELDS as [key, label] (key)}
      <div class="field">
        <label for={key}>{label}</label>
        <input id={key} class="input" bind:value={settings[key]} />
      </div>
    {/each}
    <div class="field">
      <label for="cfg-min-t">条件反映の開始位置</label>
      <input
        id="cfg-min-t"
        class="input"
        type="number"
        step="0.01"
        bind:value={settings.cfg_min_t}
      />
    </div>
    <div class="field">
      <label for="cfg-max-t">条件反映の終了位置</label>
      <input
        id="cfg-max-t"
        class="input"
        type="number"
        step="0.01"
        bind:value={settings.cfg_max_t}
      />
    </div>
    <p class="help wide">内部補正の8項目は通常は空欄にしてください。</p>
    <div class="field wide">
      <button
        class="btn unload"
        type="button"
        disabled={unloading}
        onclick={onunload}>モデルをメモリから解放</button
      >
    </div>
  </div>
</details>

<style lang="sass">
  .settings
    border: 1px solid var(--c-border)
    border-radius: var(--radius-lg)
    background: var(--c-surface-raised)

    summary
      padding: var(--sp-3) var(--sp-4)
      font-size: var(--fs-md)
      font-weight: 500
      cursor: pointer

  .settings-grid
    display: grid
    gap: var(--sp-3) var(--sp-5)
    padding: var(--sp-3) var(--sp-4) var(--sp-4)

    .wide
      grid-column: 1 / -1

  @media (min-width: 768px)
    .settings-grid
      grid-template-columns: 1fr 1fr

  .unload
    align-self: flex-start
</style>
