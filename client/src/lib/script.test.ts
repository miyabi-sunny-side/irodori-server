import { describe, expect, it } from "vitest";

import {
  estimateSeconds,
  isLong,
  makeCard,
  packScriptFile,
  parseScriptFile,
  pushHistory,
  reorder,
  splitText,
  type ScriptCard,
} from "./script";

const voices = [
  { id: 1, name: "ずんだ" },
  { id: 2, name: "あかり" },
];

describe("estimateSeconds", () => {
  it("weighs punctuation, kanji, kana, digits and Latin like EasyIrodoriTTS", () => {
    expect(estimateSeconds("。", [], 1)).toBe(0.4);
    expect(estimateSeconds("漢字", [], 1)).toBe(0.8);
    expect(estimateSeconds("かな", [], 1)).toBe(0.3);
    expect(estimateSeconds("12", [], 1)).toBe(0.5);
    expect(estimateSeconds("ab ", [], 1)).toBe(0.3);
  });

  it("reads dictionary words by their reading and stretches with the duration scale", () => {
    const dictionary = [{ word: "TTS", reading: "てぃーてぃーえす" }];
    expect(estimateSeconds("TTS", dictionary, 1)).toBe(1.3);
    expect(estimateSeconds("かな", [], 1.5)).toBe(0.5);
    expect(estimateSeconds("かな", [], 0.5)).toBe(0.3);
  });
});

describe("splitText", () => {
  it("keeps short text whole", () => {
    expect(splitText("こんにちは。", [], 1)).toEqual(["こんにちは。"]);
    expect(splitText("", [], 1)).toEqual([""]);
  });

  it("splits long text at sentence ends without losing characters", () => {
    const sentence = "今日はとても良い天気ですね、散歩に行きましょう。";
    const text = sentence.repeat(12);
    const pieces = splitText(text, [], 1);
    expect(pieces.length).toBeGreaterThan(1);
    expect(pieces.join("")).toBe(text);
    for (const piece of pieces) {
      expect(piece.endsWith("。")).toBe(true);
      expect(estimateSeconds(piece, [], 1)).toBeLessThanOrEqual(25);
    }
  });

  it("never cuts inside a dictionary word or a grapheme", () => {
    const word = "長い固有名詞";
    const dictionary = [{ word, reading: "ながいこゆうめいし" }];
    const text = `あいうえお${word}`.repeat(30);
    const pieces = splitText(text, dictionary, 1);
    expect(pieces.join("")).toBe(text);
    for (const piece of pieces) {
      expect(piece.split(word).join("")).not.toMatch(/[長固有名詞]/);
    }
    const family = "👨‍👩‍👧";
    const emoji = `${"か".repeat(150)}${family}`.repeat(2);
    expect(splitText(emoji, [], 1).join("")).toBe(emoji);
    expect(splitText(emoji, [], 1).some((p) => p.includes(family))).toBe(true);
  });

  it("refuses a dictionary reading too long for one card", () => {
    const dictionary = [{ word: "X", reading: "あ".repeat(200) }];
    expect(() => splitText("X", dictionary, 1)).toThrow("読み辞書");
  });
});

describe("isLong", () => {
  it("flags cards over 25 estimated seconds or 180 characters", () => {
    expect(isLong("かな", [], 1)).toBe(false);
    expect(isLong("、".repeat(181), [], 1)).toBe(true);
    expect(isLong("漢".repeat(70), [], 1)).toBe(true);
  });
});

describe("makeCard", () => {
  it("copies the voice, speed and silence but not the text", () => {
    const source: ScriptCard = {
      id: "a",
      text: "元",
      voice_id: 2,
      speed: 1.2,
      silence: 3,
    };
    expect(makeCard("b", source)).toEqual({
      id: "b",
      text: "",
      voice_id: 2,
      speed: 1.2,
      silence: 3,
    });
    expect(makeCard("c")).toEqual({
      id: "c",
      text: "",
      voice_id: null,
      speed: 1,
      silence: 0,
    });
  });
});

describe("reorder", () => {
  it("moves one card to another position", () => {
    expect(reorder(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(reorder(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
    expect(reorder(["a", "b"], 1, 5)).toEqual(["a", "b"]);
  });
});

describe("pushHistory", () => {
  it("keeps the latest 100 snapshots", () => {
    let stack: number[] = [];
    for (let i = 0; i < 105; i++) stack = pushHistory(stack, i);
    expect(stack).toHaveLength(100);
    expect(stack[0]).toBe(5);
    expect(stack.at(-1)).toBe(104);
  });
});

describe("script files", () => {
  const cards: ScriptCard[] = [
    { id: "a", text: "一", voice_id: 1, speed: 1.25, silence: 1 },
    { id: "b", text: "二", voice_id: 9, speed: 1, silence: 0 },
  ];
  const settings = { seed: "3", lora_adapter_raw: "/home/me/lora" };

  it("round-trips cards, settings and the dictionary switch, without machine paths", () => {
    const packed = packScriptFile(
      { cards, settings, dictionary_enabled: false },
      voices,
    );
    expect(packed.settings.lora_adapter_raw).toBe("");
    const parsed = parseScriptFile(JSON.stringify(packed), voices);
    if ("error" in parsed) throw new Error(parsed.error);
    expect(parsed.cards).toEqual([
      { text: "一", voice_id: 1, speed: 1.25, silence: 1 },
      { text: "二", voice_id: null, speed: 1, silence: 0 },
    ]);
    expect(parsed.settings?.seed).toBe("3");
    expect(parsed.dictionary_enabled).toBe(false);
    expect(parsed.missing).toBe(1);
  });

  it("matches voices by name when the id belongs to another server", () => {
    const file = JSON.stringify({
      format: "irodori-server-script",
      version: 1,
      dictionary_enabled: true,
      settings: {},
      cards: [{ text: "a", voice_id: 77, voice_name: "あかり" }],
    });
    const parsed = parseScriptFile(file, voices);
    if ("error" in parsed) throw new Error(parsed.error);
    expect(parsed.cards[0].voice_id).toBe(2);
    expect(parsed.missing).toBe(0);
  });

  it("reads EasyIrodoriTTS input files, keeping the current settings", () => {
    const file = JSON.stringify({
      format: "easy-irodori-input",
      version: 1,
      generation: { steps: 4 },
      dictionary_enabled: true,
      cards: [
        {
          text: "v2",
          voice_id: "0123abcd",
          voice_name: "ずんだ",
          speed: 0.75,
          silence: 10,
        },
      ],
    });
    const parsed = parseScriptFile(file, voices);
    if ("error" in parsed) throw new Error(parsed.error);
    expect(parsed.cards[0]).toMatchObject({
      text: "v2",
      voice_id: 1,
      speed: 0.75,
      silence: 10,
    });
    expect(parsed.settings).toBeUndefined();
  });

  it("rejects files it cannot use", () => {
    const card = { text: "a" };
    const wrap = (extra: object) =>
      JSON.stringify({
        format: "irodori-server-script",
        version: 1,
        cards: [card],
        ...extra,
      });
    for (const raw of [
      "not json",
      "[]",
      JSON.stringify({ format: "other", version: 1, cards: [card] }),
      wrap({ version: 2 }),
      wrap({ cards: [] }),
      wrap({ cards: Array.from({ length: 1001 }, () => card) }),
      wrap({ cards: [{ text: 1 }] }),
      wrap({ cards: [{ text: "a".repeat(10_001) }] }),
      wrap({ cards: [{ text: "a", speed: 2 }] }),
      wrap({ cards: [{ text: "a", silence: -1 }] }),
      wrap({ dictionary_enabled: "yes" }),
    ]) {
      expect(parseScriptFile(raw, voices), raw.slice(0, 60)).toHaveProperty(
        "error",
      );
    }
  });
});
