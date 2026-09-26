import { describe, expect, it } from "vitest";

import type { Batch, Reference } from "./api";
import {
  characterOptions,
  lineSummary,
  MAX_BATCH_LINES,
  progressText,
} from "./batch";

const batch = (extra: Partial<Batch>): Batch => ({
  id: "b1",
  character: "ずんだ",
  total: 3,
  done: 0,
  generation_ids: [],
  failed: [],
  finished: false,
  ...extra,
});

describe("lineSummary", () => {
  it("counts trimmed, non-blank lines like the server", () => {
    expect(lineSummary(" おはよう \n\n\r\nこんにちは\r\n  ")).toEqual({
      count: 2,
      error: "",
    });
  });

  it("explains an empty input and too many lines", () => {
    expect(lineSummary(" \n ").error).toBe("台詞を1行以上入力してください。");
    expect(lineSummary("あ\n".repeat(MAX_BATCH_LINES)).error).toBe("");
    expect(lineSummary("あ\n".repeat(MAX_BATCH_LINES + 1))).toEqual({
      count: MAX_BATCH_LINES + 1,
      error: `一度に生成できるのは${MAX_BATCH_LINES}行までです。`,
    });
  });
});

describe("progressText", () => {
  it("shows the running count and the final result", () => {
    expect(progressText(batch({ done: 1 }))).toBe("生成しています… 1 / 3");
    expect(
      progressText(
        batch({
          done: 3,
          finished: true,
          generation_ids: [1, 2],
          failed: [{ line: "x", error: "e" }],
        }),
      ),
    ).toBe("完了しました（成功 2件・失敗 1件）");
  });
});

describe("characterOptions", () => {
  it("lists each character once with its number of references", () => {
    const refs = [
      { id: 1, name: "生成 1", character: "あかり" },
      { id: 2, name: "生成 2", character: "あかり" },
      { id: 3, name: "生成 3", character: "ずんだ" },
      { id: 4, name: "upload.wav", character: null },
    ] as Reference[];
    expect(characterOptions(refs)).toEqual([
      { name: "あかり", count: 2 },
      { name: "ずんだ", count: 1 },
    ]);
  });
});
