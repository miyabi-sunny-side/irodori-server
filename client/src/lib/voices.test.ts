import { describe, expect, it } from "vitest";

import type { Generation } from "./api";
import {
  deletableCount,
  filterByCharacter,
  generationLabel,
  historyCharacters,
  inheritedCaption,
  referenceNote,
  voiceName,
} from "./voices";

const base = {
  batch: "b",
  candidate: 1,
  created_at: "2026-10-15T00:00:00Z",
  text: "a",
  text_applied: "a",
  mode: "design",
  caption: "明るい声",
  reference_ids: [],
  model: "m",
  seed: "1",
  speed: 1,
  audio_url: "",
  saved: false,
  character: null,
  favorite: false,
  voice_id: null,
  scripted: false,
};

function generation(id: number, extra: Partial<Generation> = {}): Generation {
  return { ...base, id, ...extra };
}

describe("voiceName", () => {
  it("trims and limits names to one line of 80 characters", () => {
    expect(voiceName("  ずんだ ")).toEqual({ value: "ずんだ" });
    expect(voiceName("あ".repeat(80))).toEqual({ value: "あ".repeat(80) });
    for (const bad of ["", "  ", "a\nb", "a\tb", "あ".repeat(81)]) {
      expect(voiceName(bad)).toHaveProperty("error");
    }
  });
});

describe("history helpers", () => {
  it("counts only audio that bulk deletion removes", () => {
    expect(
      deletableCount([
        generation(1),
        generation(2, { saved: true }),
        generation(3, { favorite: true }),
        generation(4, { scripted: true }),
      ]),
    ).toBe(1);
  });

  it("labels and filters by the voice name", () => {
    const voiced = generation(5, { character: "ずんだ" });
    expect(generationLabel(voiced)).toBe("ずんだ #5");
    expect(generationLabel(generation(6))).toBe("#6");
    expect(filterByCharacter([voiced, generation(6)], "ずんだ")).toEqual([
      voiced,
    ]);
    expect(historyCharacters([voiced, generation(6), voiced])).toEqual([
      "ずんだ",
    ]);
  });

  it("inherits the speaking style only when the generation used it", () => {
    expect(inheritedCaption(generation(1))).toBe("明るい声");
    expect(inheritedCaption(generation(1, { mode: "both" }))).toBe("明るい声");
    expect(inheritedCaption(generation(1, { mode: "clone" }))).toBe("");
    expect(inheritedCaption(generation(1, { mode: "auto" }))).toBe("");
  });
});

describe("referenceNote", () => {
  it("refuses audio outside 1–120 seconds and warns under 10", () => {
    expect(referenceNote(0.5)).toHaveProperty("error");
    expect(referenceNote(120.5)).toHaveProperty("error");
    expect(referenceNote(5)).toHaveProperty("warning");
    expect(referenceNote(10)).toEqual({});
    expect(referenceNote(Number.NaN)).toEqual({});
  });
});
