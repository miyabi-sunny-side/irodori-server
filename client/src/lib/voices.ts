// Pure helpers for voices (話者) and the history list.

import type { Generation } from "./api";

export const VOICE_NAME_MAX = 80;

/** Mirrors the server check so the form can explain the problem first. */
export function voiceName(raw: string): { value: string } | { error: string } {
  const value = raw.trim();
  if (
    !value ||
    [...value].length > VOICE_NAME_MAX ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    return {
      error: `話者名を1行・${VOICE_NAME_MAX}文字までで入力してください。`,
    };
  }
  return { value };
}

/** Generations bulk deletion removes: not starred, registered as a voice or used by the script. */
export function deletableCount(generations: Generation[]): number {
  return generations.filter((g) => !g.saved && !g.favorite && !g.scripted)
    .length;
}

/** 「話者名 #ID」, or just the ID for audio made without a voice. */
export function generationLabel(generation: Generation): string {
  return generation.character
    ? `${generation.character} #${generation.id}`
    : `#${generation.id}`;
}

/** An empty name keeps every generation. */
export function filterByCharacter(
  generations: Generation[],
  character: string,
): Generation[] {
  return character
    ? generations.filter((generation) => generation.character === character)
    : generations;
}

/** Voice names in the history, in order of appearance. */
export function historyCharacters(generations: Generation[]): string[] {
  const names = generations.map((generation) => generation.character ?? "");
  return [...new Set(names.filter(Boolean))];
}

/** The speaking style a voice registered from this generation keeps. */
export function inheritedCaption(generation: Generation): string {
  return generation.mode === "design" || generation.mode === "both"
    ? generation.caption
    : "";
}

/** Whether an uploaded reference can be registered (1–120 s), with a note under 10 s. */
export function referenceNote(seconds: number): {
  error?: string;
  warning?: string;
} {
  if (!Number.isFinite(seconds)) return {};
  if (seconds < 1 || seconds > 120) {
    return { error: "参照音声は1～120秒の音声にしてください。" };
  }
  if (seconds < 10) {
    return {
      warning:
        "10秒未満の短い音声のため、声の特徴を十分に再現できない場合があります。登録することはできます。",
    };
  }
  return {};
}
