import { describe, expect, it } from "vitest";

import {
  buildRequest,
  clampNumber,
  defaultForm,
  insertText,
  moveItem,
  parseVolume,
} from "./create";

describe("insertText", () => {
  it("inserts at the caret and moves the caret after the insertion", () => {
    expect(insertText("こんにちは", 2, 2, "😊")).toEqual({
      text: "こん😊にちは",
      caret: 4,
    });
  });

  it("replaces the selected range", () => {
    expect(insertText("abcdef", 1, 4, "X")).toEqual({ text: "aXef", caret: 2 });
  });

  it("appends when the caret is unknown or out of range", () => {
    expect(insertText("abc", null, null, "!")).toEqual({
      text: "abc!",
      caret: 4,
    });
    expect(insertText("abc", 9, 12, "!")).toEqual({ text: "abc!", caret: 4 });
  });
});

describe("moveItem", () => {
  it("moves an item up or down without mutating the input", () => {
    const list = ["a", "b", "c"];
    expect(moveItem(list, 2, -1)).toEqual(["a", "c", "b"]);
    expect(moveItem(list, 0, 1)).toEqual(["b", "a", "c"]);
    expect(list).toEqual(["a", "b", "c"]);
  });

  it("keeps the order when the move leaves the list", () => {
    expect(moveItem(["a", "b"], 0, -1)).toEqual(["a", "b"]);
    expect(moveItem(["a", "b"], 1, 1)).toEqual(["a", "b"]);
  });
});

describe("parseVolume", () => {
  it("accepts stored values from 0 to 1", () => {
    expect(parseVolume("0")).toBe(0);
    expect(parseVolume("0.4")).toBe(0.4);
    expect(parseVolume("1")).toBe(1);
  });

  it("falls back to 75% for missing or invalid values", () => {
    for (const value of [null, "", " ", "abc", "-0.1", "1.5", "NaN"]) {
      expect(parseVolume(value)).toBe(0.75);
    }
  });
});

describe("clampNumber", () => {
  it("clamps into the range and keeps the fallback for non-numbers", () => {
    expect(clampNumber(130, 1, 120, 40)).toBe(120);
    expect(clampNumber(0, 1, 120, 40)).toBe(1);
    expect(clampNumber(Number.NaN, 1, 120, 40)).toBe(40);
    expect(clampNumber(1.25, 0.75, 1.5, 1)).toBe(1.25);
  });
});

describe("buildRequest", () => {
  it("sends every field of the generate contract with the Gradio defaults", () => {
    const request = buildRequest({ ...defaultForm(), text: "こんにちは。" }, [
      { id: 3, name: "a.wav" },
    ]);
    expect(request).toEqual({
      model: "Aratako/Irodori-TTS-v4.1-Small",
      mode: "design",
      caption: "",
      reference_ids: [3],
      text: "こんにちは。",
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
    });
  });

  it("keeps the reference order and trims the free-text fields", () => {
    const form = { ...defaultForm(), seed: " 42 ", lora_adapter_raw: " " };
    const request = buildRequest(form, [
      { id: 7, name: "b.wav" },
      { id: 2, name: "a.wav" },
    ]);
    expect(request.reference_ids).toEqual([7, 2]);
    expect(request.seed).toBe("42");
    expect(request.lora_adapter_raw).toBe("");
  });
});
