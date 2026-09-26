// Pure helpers behind the 音声作成 page. Kept free of DOM and fetch so they
// can be unit-tested; the page and its module state call into them.

import type { GenerateRequest, Reference } from "./api";

export const DEFAULT_VOLUME = 0.75;

/** The editable form; references are kept separately with their names. */
export type CreateForm = Omit<GenerateRequest, "reference_ids">;

// Free-text fields the worker parses; surrounding spaces are never meant.
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

export function defaultForm(): CreateForm {
  return {
    model: "Aratako/Irodori-TTS-v4.1-Small",
    mode: "design",
    caption: "",
    text: "",
    dictionary_enabled: true,
    speed: 1,
    num_steps: 40,
    num_candidates: 1,
    duration_scale: 1,
    seed: "",
    seconds: "",
    cfg_scale_text: 3,
    cfg_scale_caption: 4,
    cfg_scale_speaker: 5,
    model_device: "cpu",
    model_precision: "fp32",
    codec_device: "cpu",
    codec_precision: "fp32",
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

export function buildRequest(
  form: CreateForm,
  references: Reference[],
): GenerateRequest {
  const trimmed = Object.fromEntries(
    TRIMMED_FIELDS.map((key) => [key, form[key].trim()]),
  );
  return {
    ...form,
    ...trimmed,
    reference_ids: references.map((reference) => reference.id),
  };
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
