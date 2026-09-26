// In-memory stand-in for the irodori-server API (docs/api.md), installed with
// page.route so the browser behaviour is tested without the Rust server.

import type { Page, Route } from "@playwright/test";

export const INFO = {
  models: [
    { id: "Aratako/Irodori-TTS-v4.1-Small", label: "ベース（標準モデル）" },
    {
      id: "phasefield-audio/Irodori-TTS-v4.1-Anime",
      label: "Anime（派生版のアニメ声強化モデル）",
    },
  ],
  modes: [
    { id: "design", label: "言葉で声を作る（ボイスデザイン）" },
    { id: "clone", label: "お手本の声に似せる（ボイスクローン）" },
    { id: "both", label: "お手本の声＋話し方を指定" },
    { id: "auto", label: "文章だけでおまかせ" },
  ],
  devices: ["cuda", "cpu"],
  precisions: { cuda: ["fp32", "bf16"], cpu: ["fp32"] },
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

/** A 0.5 s silent mono 16-bit WAV. */
export function silentWav(): Buffer {
  const rate = 8000;
  const data = Buffer.alloc(rate); // 0.5 s * 2 bytes
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

interface Generation {
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
}

export interface MockBatch {
  id: string;
  character: string;
  total: number;
  done: number;
  generation_ids: number[];
  failed: { line: string; error: string }[];
  finished: boolean;
  /** Lines still to voice; one is voiced per GET, like a slow server. */
  pending: string[];
}

export interface SavedReference {
  id: number;
  created_at: string;
  name: string;
  character: string;
  source_generation_id: number | null;
  audio_url: string;
}

export interface Backend {
  requests: Record<string, unknown>[];
  generations: Generation[];
  dictionary: { word: string; reading: string }[];
  fileErrors: { path: string; error: string; created_at: string }[];
  references: SavedReference[];
  batches: MockBatch[];
  /** Bodies sent to POST /api/batches. */
  batchRequests: Record<string, unknown>[];
  /** Resolve to let a pending POST /api/generate answer. */
  release?: () => void;
  holdGenerate: boolean;
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
    ...extra,
  };
}

export function savedReference(
  id: number,
  character: string,
  sourceGenerationId: number | null = null,
): SavedReference {
  return {
    id,
    created_at: "2026-09-26T04:05:06Z",
    name: `生成 ${sourceGenerationId ?? id}`,
    character,
    source_generation_id: sourceGenerationId,
    audio_url: `/api/references/${id}/audio`,
  };
}

function publicBatch(batch: MockBatch) {
  const { pending: _pending, ...rest } = batch;
  return rest;
}

export async function mockBackend(
  page: Page,
  initial: Partial<Backend> = {},
): Promise<Backend> {
  const backend: Backend = {
    requests: [],
    generations: [],
    dictionary: [],
    fileErrors: [],
    references: [],
    batches: [],
    batchRequests: [],
    holdGenerate: false,
    ...initial,
  };
  let nextId = 100;
  let nextReference = 1;
  const json = (route: Route, body: unknown, status = 200) =>
    route.fulfill({ status, json: body });
  const apply = (text: string) =>
    backend.dictionary.reduce(
      (out, entry) => out.split(entry.word).join(entry.reading),
      text,
    );

  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (path === "/api/info") return json(route, INFO);
    if (path === "/api/generate" && method === "POST") {
      const body = request.postDataJSON() as Record<string, unknown>;
      backend.requests.push(body);
      if (backend.holdGenerate) {
        await new Promise<void>((resolve) => (backend.release = resolve));
      }
      const count = Number(body.num_candidates);
      const made = Array.from({ length: count }, (_, index) =>
        generation(nextId++, String(body.text), {
          candidate: index + 1,
          speed: body.speed,
          mode: body.mode,
          model: body.model,
          text_applied: apply(String(body.text)),
        }),
      );
      // Newest first, like the server's list.
      backend.generations = [...made].reverse().concat(backend.generations);
      return json(route, {
        generations: made,
        log: "runtime: reloaded\nseed_used: 1",
      });
    }
    if (path === "/api/generations/cleanup" && method === "POST") {
      const before = backend.generations.length;
      backend.generations = backend.generations.filter(
        (item) => item.saved || item.favorite,
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
    if (path === "/api/batches" && method === "POST") {
      const body = request.postDataJSON() as {
        character: string;
        lines: string;
      };
      backend.batchRequests.push(body);
      if (!backend.references.some((r) => r.character === body.character)) {
        return json(
          route,
          {
            error:
              "このキャラクターのお手本がありません。生成履歴からお手本に保存してください。",
          },
          400,
        );
      }
      const lines = body.lines
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
      const batch: MockBatch = {
        id: `batch-${backend.batches.length + 1}`,
        character: body.character,
        total: lines.length,
        done: 0,
        generation_ids: [],
        failed: [],
        finished: false,
        pending: lines,
      };
      backend.batches.unshift(batch);
      return json(route, { id: batch.id, total: batch.total }, 202);
    }
    if (path === "/api/batches" && method === "GET") {
      return json(route, { batches: backend.batches.map(publicBatch) });
    }
    const batchOne = path.match(/^\/api\/batches\/(.+)$/);
    if (batchOne && method === "GET") {
      const batch = backend.batches.find((item) => item.id === batchOne[1]);
      if (!batch) return json(route, { error: "not found" }, 404);
      const line = batch.pending.shift();
      if (line !== undefined) {
        batch.done += 1;
        if (line.includes("失敗")) {
          batch.failed.push({ line, error: "生成できませんでした。" });
        } else {
          const made = generation(nextId++, line, {
            character: batch.character,
            mode: "clone",
          });
          backend.generations.unshift(made);
          batch.generation_ids.push(made.id);
        }
        batch.finished = batch.pending.length === 0;
      }
      return json(route, publicBatch(batch));
    }
    const save = path.match(/^\/api\/generations\/(\d+)\/reference$/);
    if (save && method === "POST") {
      const { character } = request.postDataJSON() as { character: string };
      const name = character.trim();
      if (!name || [...name].length > 50) {
        return json(
          route,
          { error: "キャラクター名は1行・50文字までで入力してください。" },
          400,
        );
      }
      const source = backend.generations.find(
        (item) => item.id === Number(save[1]),
      );
      if (source) source.saved = true;
      const reference = savedReference(nextReference++, name, Number(save[1]));
      backend.references.push(reference);
      backend.references.sort(
        (a, b) => a.character.localeCompare(b.character) || a.id - b.id,
      );
      return json(route, reference);
    }
    if (path === "/api/references" && method === "GET") {
      return json(route, { references: backend.references });
    }
    const referenceAudio = path.match(/^\/api\/references\/(\d+)\/audio$/);
    if (referenceAudio) {
      return route.fulfill({
        status: 200,
        headers: { "content-type": "audio/wav" },
        body: silentWav(),
      });
    }
    const reference = path.match(/^\/api\/references\/(\d+)$/);
    if (reference && method === "DELETE") {
      const id = Number(reference[1]);
      const removed = backend.references.find((item) => item.id === id);
      backend.references = backend.references.filter((item) => item.id !== id);
      const source = backend.generations.find(
        (item) => item.id === removed?.source_generation_id,
      );
      if (source) source.saved = false;
      return route.fulfill({ status: 204 });
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
    if (path === "/api/references" && method === "POST") {
      const name = /filename="([^"]+)"/.exec(
        request.postDataBuffer()?.toString("utf8") ?? "",
      )?.[1];
      return json(route, { id: nextReference++, name });
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
