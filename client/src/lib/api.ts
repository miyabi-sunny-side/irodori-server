// Typed access to the irodori-server JSON API (docs/api.md).

export interface Choice {
  id: string;
  label: string;
}

export interface EmojiItem {
  emoji: string;
  label: string;
  description: string;
}

export interface Info {
  models: Choice[];
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
  text: string;
  dictionary_enabled: boolean;
  speed: number;
  num_steps: number;
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
  /** Saved as a reference; bulk deletion keeps it. */
  saved: boolean;
}

export interface FileError {
  path: string;
  error: string;
  created_at: string;
}

/** Reference audio for voice cloning. Uploads carry only id and name. */
export interface Reference {
  id: number;
  name: string;
  character?: string | null;
  created_at?: string;
  source_generation_id?: number | null;
  audio_url?: string;
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
  saveReference: (generationId: number, character: string) =>
    request<Reference>(
      `/api/generations/${generationId}/reference`,
      sendJson("POST", { character }),
    ),
  references: (signal?: AbortSignal) =>
    request<{ references: Reference[] }>("/api/references", { signal }),
  deleteReference: (id: number) =>
    request<void>(`/api/references/${id}`, { method: "DELETE" }),
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
