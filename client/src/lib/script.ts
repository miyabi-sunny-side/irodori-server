// Pure helpers behind the 音声作成 script: estimates and splitting as in
// EasyIrodoriTTS v2.0 (easy_splitter.py), cards, undo history and the
// JSON files for 入力済データ. Kept free of DOM and fetch for unit tests.

import type { DictionaryEntry } from "./api";

export interface ScriptCard {
  id: string;
  text: string;
  voice_id: number | null;
  speed: number;
  silence: number;
}

export const MAX_CARDS = 1000;
export const MAX_CARD_CHARS = 10_000;
export const MAX_FILE_BYTES = 7 * 1024 * 1024;
export const TARGET_SECONDS = 25;
const LONG_CHARS = 180;
const FORMAT = "irodori-server-script";
const EASY_FORMAT = "easy-irodori-input";

type Unit = [raw: string, reading: string];

const graphemes = (text: string) =>
  [...new Intl.Segmenter("ja", { granularity: "grapheme" }).segment(text)].map(
    (part) => part.segment,
  );

/** The text as spans: dictionary words whole with their reading, otherwise graphemes. */
function units(text: string, dictionary: DictionaryEntry[]): Unit[] {
  const words = [...dictionary].sort((a, b) => b.word.length - a.word.length);
  const out: Unit[] = [];
  let rest = text;
  let plain = "";
  const flush = () => {
    for (const g of graphemes(plain)) out.push([g, g]);
    plain = "";
  };
  while (rest) {
    const found = words.find(
      (entry) => entry.word && rest.startsWith(entry.word),
    );
    if (found) {
      flush();
      out.push([found.word, found.reading]);
      rest = rest.slice(found.word.length);
    } else {
      const ch = String.fromCodePoint(rest.codePointAt(0)!);
      plain += ch;
      rest = rest.slice(ch.length);
    }
  }
  flush();
  return out;
}

/** A conservative heuristic, not an acoustic duration predictor. */
function seconds(text: string): number {
  let total = 0;
  for (const ch of text) {
    if ("。！？!?".includes(ch)) total += 0.38;
    else if ("、，,;；".includes(ch)) total += 0.18;
    else if (/\s/u.test(ch)) total += 0;
    else if (/[一-鿿]/u.test(ch)) total += 0.38;
    else if (/[0-9]/.test(ch)) total += 0.25;
    else if (/[A-Za-z]/.test(ch)) total += 0.13;
    else if (/[\p{L}\p{N}]/u.test(ch)) total += 0.16;
    else if (/\p{So}/u.test(ch)) total += 0.2;
  }
  return total;
}

export function estimateSeconds(
  text: string,
  dictionary: DictionaryEntry[],
  scale: number,
): number {
  const total = units(text, dictionary).reduce(
    (sum, [, reading]) => sum + seconds(reading),
    0,
  );
  return Math.round(total * Math.max(1, scale) * 10) / 10;
}

export function isLong(
  text: string,
  dictionary: DictionaryEntry[],
  scale: number,
): boolean {
  return (
    estimateSeconds(text, dictionary, scale) > TARGET_SECONDS ||
    [...text].length > LONG_CHARS
  );
}

/** Splits text into card-sized pieces that join back to the same text. */
export function splitText(
  text: string,
  dictionary: DictionaryEntry[],
  scale: number,
  target = TARGET_SECONDS,
): string[] {
  const spans = units(text, dictionary);
  if (!text.trim()) return [text];
  const limit = target / Math.max(1, scale);
  const pieces: string[] = [];
  let start = 0;
  while (start < spans.length) {
    let weight = 0;
    let end = start;
    while (end < spans.length) {
      const amount = seconds(spans[end][1]);
      if (amount > limit) {
        throw new Error(
          "読み辞書の1語が長すぎます。登録した読み方を短くしてください。",
        );
      }
      if (end > start && (weight + amount > limit || end - start >= LONG_CHARS))
        break;
      weight += amount;
      end += 1;
    }
    if (end < spans.length) {
      // Prefer natural boundaries in the latter half of this piece.
      const lower = start + Math.max(1, Math.floor((end - start) / 2));
      for (const marks of ["\n。！？!?", "、，,;；", " \t"]) {
        let found = -1;
        for (let i = lower - 1; i < end; i++) {
          if (marks.includes(spans[i][0].slice(-1))) found = i + 1;
        }
        if (found > 0) {
          end = found;
          break;
        }
      }
      // Keep a short Latin word together when possible.
      if (end < spans.length && /^[A-Za-z0-9]$/.test(spans[end][0])) {
        let boundary = end;
        while (boundary > start && /^[A-Za-z0-9]$/.test(spans[boundary - 1][0]))
          boundary -= 1;
        if (boundary > start && end - boundary < 30) end = boundary;
      }
    }
    const piece = spans
      .slice(start, end)
      .map(([raw]) => raw)
      .join("");
    if (!piece.trim() && pieces.length > 0) pieces[pieces.length - 1] += piece;
    else pieces.push(piece);
    start = end;
  }
  if (pieces.length > 1 && !pieces[0].trim()) {
    pieces[1] = pieces[0] + pieces[1];
    pieces.shift();
  }
  return pieces;
}

/** A blank card, copying the voice, speed and silence of `source` if given. */
export function makeCard(id: string, source?: ScriptCard): ScriptCard {
  return {
    id,
    text: "",
    voice_id: source?.voice_id ?? null,
    speed: source?.speed ?? 1,
    silence: source?.silence ?? 0,
  };
}

export function reorder<T>(list: T[], from: number, to: number): T[] {
  if (from < 0 || from >= list.length || to < 0 || to >= list.length) {
    return [...list];
  }
  const moved = [...list];
  const [item] = moved.splice(from, 1);
  moved.splice(to, 0, item);
  return moved;
}

export function pushHistory<T>(stack: T[], snapshot: T, limit = 100): T[] {
  return [...stack, snapshot].slice(-limit);
}

interface Voice {
  id: number;
  name: string;
}

interface ScriptInput {
  cards: ScriptCard[];
  settings: object;
  dictionary_enabled: boolean;
}

/** The 入力済データ file: inputs only, never audio or machine-specific paths. */
export function packScriptFile(script: ScriptInput, voices: Voice[]) {
  const names = new Map(voices.map((voice) => [voice.id, voice.name]));
  return {
    format: FORMAT,
    version: 1,
    settings: { ...script.settings, lora_adapter_raw: "" },
    dictionary_enabled: script.dictionary_enabled,
    cards: script.cards.map((card) => ({
      text: card.text,
      voice_id: card.voice_id,
      voice_name:
        card.voice_id === null ? "" : (names.get(card.voice_id) ?? ""),
      speed: card.speed,
      silence: card.silence,
    })),
  };
}

export type ParsedFile =
  | {
      cards: Omit<ScriptCard, "id">[];
      settings?: Record<string, unknown>;
      dictionary_enabled: boolean;
      /** Cards whose voice is not registered here. */
      missing: number;
    }
  | { error: string };

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const inRange = (value: unknown, min: number, max: number) =>
  typeof value === "number" && value >= min && value <= max;

/**
 * Reads a file saved by this screen or by EasyIrodoriTTS v2.0. Voices match by id,
 * then by a unique name; EasyIrodoriTTS generation settings are not carried over.
 */
export function parseScriptFile(raw: string, voices: Voice[]): ParsedFile {
  const invalid = {
    error:
      "入力済データを読み込めません。このアプリで保存したファイルを選んでください。",
  };
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return invalid;
  }
  if (
    !isObject(data) ||
    ![FORMAT, EASY_FORMAT].includes(String(data.format)) ||
    data.version !== 1
  ) {
    return invalid;
  }
  const own = data.format === FORMAT;
  const items = data.cards;
  if (!Array.isArray(items) || items.length < 1 || items.length > MAX_CARDS) {
    return { error: `入力済データは1～${MAX_CARDS}枚のセリフにしてください。` };
  }
  const dictionary = data.dictionary_enabled ?? true;
  if (
    typeof dictionary !== "boolean" ||
    (own && !isObject(data.settings ?? {}))
  ) {
    return { error: "保存された設定の形式が不正です。" };
  }
  const ids = new Set(voices.map((voice) => voice.id));
  const byName = (name: string) => {
    const found = voices.filter((voice) => voice.name === name);
    return found.length === 1 ? found[0].id : null;
  };
  let missing = 0;
  const cards = [];
  for (const item of items) {
    if (
      !isObject(item) ||
      typeof item.text !== "string" ||
      [...item.text].length > MAX_CARD_CHARS
    ) {
      return { error: "セリフの形式または文章の長さが不正です。" };
    }
    const speed = item.speed ?? 1;
    const silence = item.silence ?? 0;
    if (!inRange(speed, 0.75, 1.5) || !inRange(silence, 0, 10)) {
      return { error: "話速または末尾の無音が不正です。" };
    }
    const name = typeof item.voice_name === "string" ? item.voice_name : "";
    const voice =
      typeof item.voice_id === "number" && ids.has(item.voice_id)
        ? item.voice_id
        : name
          ? byName(name)
          : null;
    if (voice === null && (name || (item.voice_id ?? "") !== "")) missing += 1;
    cards.push({
      text: item.text,
      voice_id: voice,
      speed: speed as number,
      silence: silence as number,
    });
  }
  return {
    cards,
    settings: own
      ? ((data.settings as Record<string, unknown>) ?? {})
      : undefined,
    dictionary_enabled: dictionary,
    missing,
  };
}
