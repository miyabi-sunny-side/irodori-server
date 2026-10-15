// In-memory stand-in for the irodori-server API (docs/api.md), installed with
// page.route so the browser behaviour is tested without the Rust server.

import type { Page, Route } from "@playwright/test";

const MF = "Aratako/Irodori-TTS-v4.1-Small-MF";

export const INFO = {
  models: [
    {
      id: "Aratako/Irodori-TTS-v4.1-Small",
      label: "標準",
      steps: 40,
      meanflow: false,
      quantized: false,
    },
    {
      id: "phasefield-audio/Irodori-TTS-v4.1-Anime",
      label: "Anime",
      steps: 40,
      meanflow: false,
      quantized: false,
    },
    { id: MF, label: "MF（高速）", steps: 4, meanflow: true, quantized: false },
  ],
  modes: [
    { id: "design", label: "言葉で声を作る（ボイスデザイン）" },
    { id: "clone", label: "お手本の声に似せる（ボイスクローン）" },
    { id: "both", label: "お手本の声＋話し方を指定" },
    { id: "auto", label: "文章だけでおまかせ" },
  ],
  devices: ["cuda", "cpu"],
  precisions: { cuda: ["bf16", "fp32"], cpu: ["fp32"] },
  cuda_capability: [8, 6],
  max_candidates: 32,
  emoji_groups: [
    {
      title: "ポジティブ",
      items: [
        { emoji: "😊", label: "楽しげ", description: "楽しそうに" },
        { emoji: "🤭", label: "笑い", description: "くすくす" },
      ],
    },
    {
      title: "話し方・演出",
      items: [{ emoji: "👂", label: "囁き", description: "耳元の音" }],
    },
  ],
};

/** A silent mono 16-bit WAV, 0.5 s unless given. */
export function silentWav(seconds = 0.5): Buffer {
  const rate = 8000;
  const data = Buffer.alloc(Math.round(rate * seconds) * 2);
  const header = Buffer.alloc(44);
  header.write("RIFF", 0);
  header.writeUInt32LE(36 + data.length, 4);
  header.write("WAVE", 8);
  header.write("fmt ", 12);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(1, 22);
  header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(rate * 2, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write("data", 36);
  header.writeUInt32LE(data.length, 40);
  return Buffer.concat([header, data]);
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
  seed: string;
  speed: number;
  audio_url: string;
  saved: boolean;
  character: string | null;
  favorite: boolean;
  voice_id: number | null;
  scripted: boolean;
}

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

export interface Card {
  id: string;
  text: string;
  voice_id: number | null;
  speed: number;
  silence: number;
}

interface Result {
  generation_id: number | null;
  signature: string;
  error: string;
}

export interface Backend {
  requests: Record<string, unknown>[];
  generations: Generation[];
  voices: Voice[];
  dictionary: { word: string; reading: string }[];
  fileErrors: { path: string; error: string; created_at: string }[];
  /** Bodies sent to POST/PATCH /api/voices. */
  voiceRequests: Record<string, unknown>[];
  /** Bodies sent to PUT /api/script. */
  saves: Record<string, unknown>[];
  revision: number;
  cards: Card[];
  options: { inherit: boolean; pattern: string; txt: boolean };
  settings: Record<string, unknown>;
  dictionaryEnabled: boolean;
  results: Record<string, Result>;
  /** Cards still to generate; one is generated per GET /api/script. */
  pending: string[];
  job: Record<string, unknown> | null;
  /** Answer the next PUT /api/script with 409, as after another screen's edit. */
  conflictNext: boolean;
  /** Length of uploaded and registered reference audio. */
  referenceSeconds: number;
}

export function generation(id: number, text: string, extra = {}): Generation {
  return {
    id,
    batch: `batch-${id}`,
    candidate: 1,
    created_at: "2026-09-26T03:04:05Z",
    text,
    text_applied: text,
    mode: "design",
    caption: "",
    reference_ids: [],
    model: "Aratako/Irodori-TTS-v4.1-Small",
    seed: "1",
    speed: 1,
    audio_url: `/api/generations/${id}/audio`,
    saved: false,
    character: null,
    favorite: false,
    voice_id: null,
    scripted: false,
    ...extra,
  };
}

export function voice(id: number, name: string, extra = {}): Voice {
  return {
    id,
    created_at: "2026-10-15T04:05:06Z",
    name,
    model: MF,
    caption: "",
    source_generation_id: null,
    revision: 1,
    audio_url: `/api/voices/${id}/audio`,
    ...extra,
  };
}

export function card(id: string, text: string, extra = {}): Card {
  return { id, text, voice_id: null, speed: 1, silence: 0, ...extra };
}

const signature = (c: Card) =>
  JSON.stringify([c.text, c.voice_id, c.speed, c.silence]);

export async function mockBackend(
  page: Page,
  initial: Partial<Backend> = {},
): Promise<Backend> {
  const backend: Backend = {
    requests: [],
    generations: [],
    voices: [],
    dictionary: [],
    fileErrors: [],
    voiceRequests: [],
    saves: [],
    revision: 1,
    cards: [card("c1", "")],
    options: { inherit: true, pattern: "$連番$_$話者名$_$日付$", txt: false },
    settings: { num_steps: null, duration_scale: 1, seed: "" },
    dictionaryEnabled: true,
    results: {},
    pending: [],
    job: null,
    conflictNext: false,
    referenceSeconds: 12,
    ...initial,
  };
  let nextId = 100;
  let nextVoice = 50;
  let nextReference = 1;
  const json = (route: Route, body: unknown, status = 200) =>
    route.fulfill({ status, json: body });
  const apply = (text: string) =>
    backend.dictionary.reduce(
      (out, entry) => out.split(entry.word).join(entry.reading),
      text,
    );

  const status = (c: Card) => {
    const job = backend.job;
    if (job && !job.finished) {
      if (backend.pending[0] === c.id) return "running";
      if (backend.pending.includes(c.id)) return "queued";
    }
    if (!c.text.trim()) return "empty";
    if (!backend.voices.some((v) => v.id === c.voice_id)) return "no_voice";
    const result = backend.results[c.id];
    if (result?.generation_id) {
      return result.signature === signature(c) ? "done" : "changed";
    }
    return result?.error ? "failed" : "new";
  };

  const scriptState = () => ({
    revision: backend.revision,
    script: {
      cards: backend.cards,
      settings: backend.settings,
      dictionary_enabled: backend.dictionaryEnabled,
      options: backend.options,
    },
    cards: backend.cards.map((c, index) => {
      const id = backend.results[c.id]?.generation_id ?? null;
      const name = backend.voices.find((v) => v.id === c.voice_id)?.name;
      return {
        id: c.id,
        status: status(c),
        error: backend.results[c.id]?.error ?? "",
        generation_id: id,
        audio_url: id ? `/api/generations/${id}/audio` : null,
        download_name: id
          ? `${String(index + 1).padStart(3, "0")}_${name}_20261015.wav`
          : null,
      };
    }),
    job: backend.job,
  });

  /** Generates the next pending card, like a server finishing one per poll. */
  const advance = () => {
    const job = backend.job;
    if (!job || job.finished) return;
    const id = backend.pending.shift();
    const c = backend.cards.find((item) => item.id === id);
    if (c) {
      if (c.text.includes("失敗")) {
        backend.results[c.id] = {
          ...(backend.results[c.id] ?? { generation_id: null, signature: "" }),
          error: "生成できませんでした。",
        };
        job.failed = Number(job.failed) + 1;
      } else {
        const made = generation(nextId++, c.text, {
          character: backend.voices.find((v) => v.id === c.voice_id)?.name,
          voice_id: c.voice_id,
          scripted: true,
        });
        backend.generations.unshift(made);
        backend.results[c.id] = {
          generation_id: made.id,
          signature: signature(c),
          error: "",
        };
      }
      job.done = Number(job.done) + 1;
    }
    job.pending = [...backend.pending];
    if (backend.pending.length === 0 || job.stopping) {
      backend.pending = [];
      job.pending = [];
      job.finished = true;
    }
  };

  const ready = (from?: string | null) => {
    const states = scriptState().cards;
    const start = from ? states.findIndex((c) => c.id === from) : 0;
    const rest = states.slice(start);
    const missing = rest
      .map((c, i) => [c, start + i + 1] as const)
      .filter(([c]) => c.status !== "empty" && c.status !== "done")
      .map(([, n]) => n);
    if (missing.length > 0) {
      return {
        error: `セリフ ${missing.join("・")} に、現在の入力内容に対応する音声がありません。未生成・変更分を生成してから操作してください。`,
      };
    }
    return { items: rest.filter((c) => c.status === "done") };
  };

  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (path === "/api/info") return json(route, INFO);
    if (path === "/api/generate" && method === "POST") {
      const body = request.postDataJSON() as Record<string, unknown>;
      backend.requests.push(body);
      const count = Number(body.num_candidates);
      const made = Array.from({ length: count }, (_, index) =>
        generation(nextId++, String(body.text), {
          candidate: index + 1,
          mode: body.mode,
          model: body.model,
          caption: body.caption,
          text_applied: apply(String(body.text)),
        }),
      );
      backend.generations = [...made].reverse().concat(backend.generations);
      return json(route, { generations: made, log: "seed_used: 1" });
    }
    if (path === "/api/script" && method === "GET") {
      advance();
      return json(route, scriptState());
    }
    if (path === "/api/script" && method === "PUT") {
      const body = request.postDataJSON() as {
        revision: number;
        script: {
          cards: Card[];
          settings: Record<string, unknown>;
          dictionary_enabled: boolean;
          options: Backend["options"];
        };
      };
      backend.saves.push(body);
      if (backend.conflictNext || body.revision !== backend.revision) {
        backend.conflictNext = false;
        backend.revision += 1;
        return json(
          route,
          {
            error:
              "別の画面で台本が更新されました。最新の台本を読み込み直してから操作してください。",
          },
          409,
        );
      }
      backend.revision += 1;
      backend.cards = body.script.cards;
      backend.settings = body.script.settings;
      backend.dictionaryEnabled = body.script.dictionary_enabled;
      backend.options = body.script.options;
      return json(route, scriptState());
    }
    if (path === "/api/script/generate" && method === "POST") {
      const { target } = request.postDataJSON() as { target: string };
      const states = scriptState().cards;
      const ids = backend.cards
        .filter((c, i) => {
          const s = states[i].status;
          if (s === "empty") return false;
          if (target === "all") return true;
          if (target === "changed") return s !== "done";
          return c.id === target;
        })
        .map((c) => c.id);
      backend.pending = ids;
      backend.job = {
        pending: [...ids],
        running: true,
        total: ids.length,
        done: 0,
        failed: 0,
        stopping: false,
        finished: false,
      };
      return json(route, scriptState());
    }
    if (path === "/api/script/stop" && method === "POST") {
      if (backend.job && !backend.job.finished) {
        backend.job.stopping = true;
        backend.pending = backend.pending.slice(0, 1);
      }
      return json(route, scriptState());
    }
    if (path === "/api/script/playlist") {
      const result = ready(url.searchParams.get("from"));
      if ("error" in result) return json(route, result, 409);
      return json(route, {
        items: result.items.map((c) => ({ id: c.id, audio_url: c.audio_url })),
      });
    }
    if (path === "/api/script/export") {
      const result = ready();
      if ("error" in result) return json(route, result, 409);
      return route.fulfill({
        status: 200,
        headers: { "content-type": "application/zip" },
        body: Buffer.from("PK"),
      });
    }
    if (path === "/api/voices" && method === "GET") {
      return json(route, { voices: backend.voices });
    }
    if (path === "/api/voices" && method === "POST") {
      const body = request.postDataJSON() as Record<string, unknown>;
      backend.voiceRequests.push(body);
      const name = String(body.name).trim();
      const made = voice(nextVoice++, name, {
        model: body.model,
        caption: body.caption,
        source_generation_id: body.generation_id ?? null,
      });
      backend.voices.push(made);
      const source = backend.generations.find(
        (g) => g.id === body.generation_id,
      );
      if (source) source.saved = true;
      return json(route, made);
    }
    const oneVoice = path.match(/^\/api\/voices\/(\d+)$/);
    if (oneVoice && method === "PATCH") {
      const body = request.postDataJSON() as {
        name: string;
        revision: number;
      };
      backend.voiceRequests.push(body);
      const target = backend.voices.find((v) => v.id === Number(oneVoice[1]));
      if (!target) return json(route, { error: "話者が見つかりません。" }, 404);
      if (target.revision !== body.revision) {
        return json(
          route,
          { error: "話者が更新されています。一覧から読み直してください。" },
          409,
        );
      }
      target.name = body.name;
      target.revision += 1;
      return json(route, target);
    }
    if (oneVoice && method === "DELETE") {
      backend.voices = backend.voices.filter(
        (v) => v.id !== Number(oneVoice[1]),
      );
      return route.fulfill({ status: 204 });
    }
    if (/^\/api\/(voices|references)\/\d+\/audio$/.test(path)) {
      return route.fulfill({
        status: 200,
        headers: { "content-type": "audio/wav" },
        body: silentWav(backend.referenceSeconds),
      });
    }
    if (path === "/api/references" && method === "POST") {
      const name = /filename="([^"]+)"/.exec(
        request.postDataBuffer()?.toString("utf8") ?? "",
      )?.[1];
      const id = nextReference++;
      return json(route, {
        id,
        name,
        audio_url: `/api/references/${id}/audio`,
      });
    }
    if (path === "/api/generations/cleanup" && method === "POST") {
      const before = backend.generations.length;
      backend.generations = backend.generations.filter(
        (item) => item.saved || item.favorite || item.scripted,
      );
      return json(route, { deleted: before - backend.generations.length });
    }
    const favorite = path.match(/^\/api\/generations\/(\d+)\/favorite$/);
    if (favorite && method === "PUT") {
      const target = backend.generations.find(
        (item) => item.id === Number(favorite[1]),
      );
      if (!target) {
        return json(route, { error: "生成の記録が見つかりません。" }, 404);
      }
      target.favorite = (
        request.postDataJSON() as { favorite: boolean }
      ).favorite;
      return json(route, target);
    }
    if (path === "/api/generations" && method === "GET") {
      return json(route, {
        generations: backend.generations,
        file_errors: backend.fileErrors,
      });
    }
    const audio = path.match(/^\/api\/generations\/(\d+)\/audio$/);
    if (audio) {
      const headers: Record<string, string> = { "content-type": "audio/wav" };
      if (url.searchParams.get("download") === "1") {
        headers["content-disposition"] =
          `attachment; filename="irodori-${audio[1]}.wav"`;
      }
      return route.fulfill({ status: 200, headers, body: silentWav() });
    }
    const one = path.match(/^\/api\/generations\/(\d+)$/);
    if (one && method === "DELETE") {
      backend.generations = backend.generations.filter(
        (item) => item.id !== Number(one[1]),
      );
      return route.fulfill({ status: 204 });
    }
    if (path === "/api/dictionary" && method === "GET") {
      return json(route, { entries: backend.dictionary });
    }
    if (path === "/api/dictionary" && method === "PUT") {
      const { word, reading } = request.postDataJSON() as {
        word: string;
        reading: string;
      };
      if (!word.trim() || !reading.trim()) {
        return json(
          route,
          { error: "「表記」と「読み方」の両方を入力してください。" },
          400,
        );
      }
      const existing = backend.dictionary.find((entry) => entry.word === word);
      if (existing) existing.reading = reading;
      else backend.dictionary.push({ word, reading });
      return json(route, {
        message: existing ? "読み方を更新しました。" : "読み方を登録しました。",
        entries: backend.dictionary,
      });
    }
    const entry = path.match(/^\/api\/dictionary\/(.+)$/);
    if (entry && method === "DELETE") {
      const word = decodeURIComponent(entry[1]);
      backend.dictionary = backend.dictionary.filter((e) => e.word !== word);
      return json(route, {
        message: "選んだ登録を削除しました。",
        entries: backend.dictionary,
      });
    }
    if (path === "/api/dictionary/preview") {
      const { text, enabled } = request.postDataJSON() as {
        text: string;
        enabled: boolean;
      };
      return json(route, { text: enabled ? apply(text) : text });
    }
    if (path === "/api/unload") {
      return json(route, {
        message:
          "モデルをメモリから解放しました。次回生成時に再読み込みします。",
      });
    }
    return json(route, { error: "not mocked" }, 404);
  });
  return backend;
}
