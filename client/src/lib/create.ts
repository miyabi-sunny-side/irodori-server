// Pure helpers behind the 音声作成 page. Kept free of DOM and fetch so they
// can be unit-tested; the page and its module state call into them.

import type { GenerateRequest } from "./api";

export const DEFAULT_VOLUME = 0.75;

// Free-text fields the engine parses; surrounding spaces are never meant.
const TRIMMED_FIELDS = [
  "seed",
  "seconds",
  "cfg_scale_raw",
  "speaker_kv_scale_raw",
  "max_text_len_raw",
  "max_caption_len_raw",
  "truncation_factor_raw",
  "rescale_k_raw",
  "rescale_sigma_raw",
  "lora_adapter_raw",
] as const;

/** The generate contract with the Gradio version's initial values. */
export function defaultRequest(): GenerateRequest {
  return {
    model: "Aratako/Irodori-TTS-v4.1-Small",
    mode: "design",
    caption: "",
    reference_ids: [],
    voice_id: null,
    text: "",
    dictionary_enabled: true,
    speed: 1,
    silence: 0,
    num_steps: null,
    num_candidates: 1,
    duration_scale: 1,
    seed: "",
    seconds: "",
    cfg_scale_text: 3,
    cfg_scale_caption: 4,
    cfg_scale_speaker: 5,
    model_device: "",
    model_precision: "",
    codec_device: "",
    codec_precision: "",
    t_schedule_mode: "linear",
    sway_coeff: -1,
    cfg_guidance_mode: "independent",
    context_kv_cache: true,
    cfg_scale_raw: "",
    speaker_kv_scale_raw: "",
    max_text_len_raw: "",
    max_caption_len_raw: "",
    truncation_factor_raw: "",
    rescale_k_raw: "",
    rescale_sigma_raw: "",
    lora_adapter_raw: "",
    cfg_min_t: 0.5,
    cfg_max_t: 1,
  };
}

/** Trims the free-text fields before a request is sent. */
export function trimRequest(request: GenerateRequest): GenerateRequest {
  const trimmed = Object.fromEntries(
    TRIMMED_FIELDS.map((key) => [key, request[key].trim()]),
  );
  return { ...request, ...trimmed };
}

/** A blank step count means the model's own; otherwise 1–120. */
export function parseSteps(raw: string): number | null {
  const value = Number.parseInt(raw, 10);
  return Number.isFinite(value) ? Math.min(120, Math.max(1, value)) : null;
}

export function insertText(
  text: string,
  start: number | null,
  end: number | null,
  insert: string,
): { text: string; caret: number } {
  const from = Math.min(start ?? text.length, text.length);
  const to = Math.min(Math.max(end ?? from, from), text.length);
  return {
    text: text.slice(0, from) + insert + text.slice(to),
    caret: from + insert.length,
  };
}

export function moveItem<T>(list: T[], index: number, delta: number): T[] {
  const target = index + delta;
  if (target < 0 || target >= list.length) {
    return [...list];
  }
  const moved = [...list];
  [moved[index], moved[target]] = [moved[target], moved[index]];
  return moved;
}

export function parseVolume(stored: string | null): number {
  if (stored === null || stored.trim() === "") {
    return DEFAULT_VOLUME;
  }
  const value = Number(stored);
  return Number.isFinite(value) && value >= 0 && value <= 1
    ? value
    : DEFAULT_VOLUME;
}

export function clampNumber(
  value: number,
  min: number,
  max: number,
  fallback: number,
): number {
  return Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}
