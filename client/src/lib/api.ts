// Typed access to the irodori-server JSON API (docs/api.md).

import type { ScriptCard } from "./script";

export interface Choice {
  id: string;
  label: string;
}

export interface EmojiItem {
  emoji: string;
  label: string;
  description: string;
}

/** A model and what the screen needs to know about it (one table on the server). */
export interface Model extends Choice {
  steps: number;
  /** MeanFlow models ignore CFG and the Sway schedule. */
  meanflow: boolean;
  /** Quantized models run only on CUDA in bf16. */
  quantized: boolean;
}

export interface Info {
  models: Model[];
  modes: Choice[];
  devices: string[];
  precisions: Record<string, string[]>;
  max_candidates: number;
  emoji_groups: { title: string; items: EmojiItem[] }[];
}

export interface GenerateRequest {
  model: string;
  mode: string;
  caption: string;
  reference_ids: number[];
  voice_id: number | null;
  text: string;
  dictionary_enabled: boolean;
  speed: number;
  silence: number;
  /** Blank takes the model's own steps. */
  num_steps: number | null;
  num_candidates: number;
  duration_scale: number;
  seed: string;
  seconds: string;
  cfg_scale_text: number;
  cfg_scale_caption: number;
  cfg_scale_speaker: number;
  model_device: string;
  model_precision: string;
  codec_device: string;
  codec_precision: string;
  t_schedule_mode: string;
  sway_coeff: number;
  cfg_guidance_mode: string;
  context_kv_cache: boolean;
  cfg_scale_raw: string;
  speaker_kv_scale_raw: string;
  max_text_len_raw: string;
  max_caption_len_raw: string;
  truncation_factor_raw: string;
  rescale_k_raw: string;
  rescale_sigma_raw: string;
  lora_adapter_raw: string;
  cfg_min_t: number;
  cfg_max_t: number;
}

export interface Generation {
  id: number;
  batch: string;
  candidate: number;
  created_at: string;
  text: string;
  text_applied: string;
  mode: string;
  caption: string;
  reference_ids: number[];
  model: string;
  seed: string | null;
  speed: number;
  audio_url: string;
  /** Registered as a voice; bulk deletion keeps it. */
  saved: boolean;
  /** The voice's name when a registered voice spoke it. */
  character: string | null;
  /** Marked with ★; bulk deletion keeps it. */
  favorite: boolean;
  voice_id: number | null;
  /** A script card's current audio; bulk deletion keeps it. */
  scripted: boolean;
}

/** A registered voice (話者): name, model, speaking style and one reference. */
export interface Voice {
  id: number;
  created_at: string;
  name: string;
  model: string;
  caption: string;
  source_generation_id: number | null;
  revision: number;
  audio_url: string;
}

export type CardStatus =
  | "empty"
  | "no_voice"
  | "new"
  | "queued"
  | "running"
  | "done"
  | "changed"
  | "failed";

/** One card's state as the server sees the saved script. */
export interface CardView {
  id: string;
  status: CardStatus;
  error: string;
  generation_id: number | null;
  audio_url: string | null;
  download_name: string | null;
}

export interface ScriptOptions {
  inherit: boolean;
  pattern: string;
  txt: boolean;
}

export interface Script {
  cards: ScriptCard[];
  settings: GenerateRequest;
  dictionary_enabled: boolean;
  options: ScriptOptions;
}

export interface ScriptJob {
  pending: string[];
  running: boolean;
  total: number;
  done: number;
  failed: number;
  stopping: boolean;
  finished: boolean;
}

export interface ScriptState {
  revision: number;
  script: Script;
  cards: CardView[];
  job: ScriptJob | null;
}

export interface FileError {
  path: string;
  error: string;
  created_at: string;
}

/** An uploaded reference audio. */
export interface Reference {
  id: number;
  name: string;
  audio_url: string;
}

export interface DictionaryEntry {
  word: string;
  reading: string;
}

/** A failed request; `message` is already written for the user. */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly log = "",
  ) {
    super(message);
  }
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, init);
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw error;
    }
    throw new ApiError(
      "サーバーに接続できませんでした。サーバーが動いているか確認して、もう一度お試しください。",
    );
  }
  if (response.status === 204) {
    return undefined as T;
  }
  const body = (await response.json().catch(() => ({}))) as {
    error?: string;
    log?: string;
  };
  if (!response.ok) {
    throw new ApiError(
      body.error ??
        `サーバーでエラーが発生しました (HTTP ${response.status})。`,
      body.log ?? "",
    );
  }
  return body as T;
}

function sendJson(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  };
}

export const api = {
  info: (signal?: AbortSignal) => request<Info>("/api/info", { signal }),
  generate: (body: GenerateRequest) =>
    request<{ generations: Generation[]; log: string }>(
      "/api/generate",
      sendJson("POST", body),
    ),
  generations: (signal?: AbortSignal) =>
    request<{ generations: Generation[]; file_errors: FileError[] }>(
      "/api/generations",
      { signal },
    ),
  deleteGeneration: (id: number) =>
    request<void>(`/api/generations/${id}`, { method: "DELETE" }),
  cleanupGenerations: () =>
    request<{ deleted: number }>("/api/generations/cleanup", {
      method: "POST",
    }),
  setFavorite: (id: number, favorite: boolean) =>
    request<Generation>(
      `/api/generations/${id}/favorite`,
      sendJson("PUT", { favorite }),
    ),
  voices: (signal?: AbortSignal) =>
    request<{ voices: Voice[] }>("/api/voices", { signal }),
  addVoice: (body: {
    name: string;
    model: string;
    caption: string;
    generation_id?: number;
    reference_id?: number;
  }) => request<Voice>("/api/voices", sendJson("POST", body)),
  renameVoice: (voice: Voice, name: string) =>
    request<Voice>(
      `/api/voices/${voice.id}`,
      sendJson("PATCH", { name, revision: voice.revision }),
    ),
  deleteVoice: (id: number) =>
    request<void>(`/api/voices/${id}`, { method: "DELETE" }),
  script: (signal?: AbortSignal) =>
    request<ScriptState>("/api/script", { signal }),
  saveScript: (revision: number, script: Script) =>
    request<ScriptState>("/api/script", sendJson("PUT", { revision, script })),
  generateScript: (revision: number, target: string) =>
    request<ScriptState>(
      "/api/script/generate",
      sendJson("POST", { revision, target }),
    ),
  stopScript: () =>
    request<ScriptState>("/api/script/stop", { method: "POST" }),
  playlist: (from?: string) =>
    request<{ items: { id: string; audio_url: string }[] }>(
      `/api/script/playlist${from ? `?from=${encodeURIComponent(from)}` : ""}`,
    ),
  /** The joined WAV or the ZIP of every card, as a file for the browser to save. */
  exportScript: async (kind: "wav" | "zip"): Promise<Blob> => {
    let response: Response;
    try {
      response = await fetch(`/api/script/export?kind=${kind}`);
    } catch {
      throw new ApiError(
        "サーバーに接続できませんでした。サーバーが動いているか確認して、もう一度お試しください。",
      );
    }
    if (!response.ok) {
      const body = (await response.json().catch(() => ({}))) as {
        error?: string;
      };
      throw new ApiError(body.error ?? "保存できませんでした。");
    }
    return response.blob();
  },
  uploadReference: (file: File) => {
    const form = new FormData();
    form.append("file", file);
    return request<Reference>("/api/references", {
      method: "POST",
      body: form,
    });
  },
  dictionary: (signal?: AbortSignal) =>
    request<{ entries: DictionaryEntry[] }>("/api/dictionary", { signal }),
  saveEntry: (word: string, reading: string) =>
    request<{ message: string; entries: DictionaryEntry[] }>(
      "/api/dictionary",
      sendJson("PUT", { word, reading }),
    ),
  deleteEntry: (word: string) =>
    request<{ message: string; entries: DictionaryEntry[] }>(
      `/api/dictionary/${encodeURIComponent(word)}`,
      { method: "DELETE" },
    ),
  preview: (text: string, enabled: boolean) =>
    request<{ text: string }>(
      "/api/dictionary/preview",
      sendJson("POST", { text, enabled }),
    ),
  unload: () => request<{ message: string }>("/api/unload", { method: "POST" }),
};

export function downloadUrl(generation: Generation): string {
  return `${generation.audio_url}?download=1`;
}
