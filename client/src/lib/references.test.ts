import { describe, expect, it } from "vitest";

import type { Generation, Reference } from "./api";
import {
  addReference,
  characterName,
  groupByCharacter,
  referenceLabel,
  unsavedCount,
} from "./references";

const ref = (id: number, character: string | null, name = `生成 ${id}`) =>
  ({
    id,
    name,
    character,
    created_at: "2026-09-26T00:00:00Z",
    source_generation_id: id,
    audio_url: `/api/references/${id}/audio`,
  }) as Reference;

describe("characterName", () => {
  it("trims and accepts a single line up to 50 characters", () => {
    expect(characterName("  ずんだ  ")).toEqual({ value: "ずんだ" });
    expect(characterName("あ".repeat(50))).toEqual({ value: "あ".repeat(50) });
  });

  it("rejects blank, multi-line, control characters and long names", () => {
    for (const bad of ["", "   ", "a\nb", "a\tb", "あ".repeat(51)]) {
      expect(characterName(bad)).toHaveProperty("error");
    }
  });
});

describe("unsavedCount", () => {
  it("counts generations that bulk deletion would remove", () => {
    const generations = [{ saved: true }, { saved: false }, { saved: false }];
    expect(unsavedCount(generations as Generation[])).toBe(2);
    expect(unsavedCount([])).toBe(0);
  });
});

describe("groupByCharacter", () => {
  it("keeps the server order and groups by character name", () => {
    const groups = groupByCharacter([
      ref(1, "あかり"),
      ref(3, "あかり"),
      ref(2, "ずんだ"),
    ]);
    expect(groups.map((g) => [g.character, g.items.map((r) => r.id)])).toEqual([
      ["あかり", [1, 3]],
      ["ずんだ", [2]],
    ]);
  });
});

describe("addReference", () => {
  it("appends a reference once", () => {
    const list = [ref(1, "あかり")];
    expect(addReference(list, ref(2, "ずんだ")).map((r) => r.id)).toEqual([
      1, 2,
    ]);
    expect(addReference(list, ref(1, "あかり"))).toEqual(list);
  });
});

describe("referenceLabel", () => {
  it("prefixes saved references with their character", () => {
    expect(referenceLabel(ref(4, "ずんだ"))).toBe("ずんだ / 生成 4");
    expect(referenceLabel({ id: 9, name: "voice.m4a" })).toBe("voice.m4a");
  });
});
