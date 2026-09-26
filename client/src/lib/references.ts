// Pure helpers for saved references (お手本) and bulk deletion of the history.

import type { Generation, Reference } from "./api";

export const CHARACTER_MAX = 50;

/** Mirrors the server check so the save dialog can explain the problem first. */
export function characterName(
  raw: string,
): { value: string } | { error: string } {
  const value = raw.trim();
  if (
    !value ||
    [...value].length > CHARACTER_MAX ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    return {
      error: `キャラクター名は1行・${CHARACTER_MAX}文字までで入力してください。`,
    };
  }
  return { value };
}

/** Generations that 「保存していない音声を一括削除」 would remove. */
export function unsavedCount(generations: Generation[]): number {
  return generations.filter((generation) => !generation.saved).length;
}

/** Groups references by character, keeping the order the server sent. */
export function groupByCharacter(
  references: Reference[],
): { character: string; items: Reference[] }[] {
  const groups: { character: string; items: Reference[] }[] = [];
  for (const reference of references) {
    const character = reference.character ?? "";
    const last = groups.at(-1);
    if (last && last.character === character) {
      last.items.push(reference);
    } else {
      groups.push({ character, items: [reference] });
    }
  }
  return groups;
}

export function addReference(
  list: Reference[],
  reference: Reference,
): Reference[] {
  return list.some((item) => item.id === reference.id)
    ? list
    : [...list, reference];
}

export function referenceLabel(reference: Reference): string {
  return reference.character
    ? `${reference.character} / ${reference.name}`
    : reference.name;
}
