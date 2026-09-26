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

/** Generations bulk deletion would remove: neither starred nor saved as a reference. */
export function deletableCount(generations: Generation[]): number {
  return generations.filter(
    (generation) => !generation.saved && !generation.favorite,
  ).length;
}

/** 「キャラクター名 #ID」, or just the ID for audio without a character. */
export function generationLabel(generation: Generation): string {
  return generation.character
    ? `${generation.character} #${generation.id}`
    : `#${generation.id}`;
}

/** An empty character keeps every generation. */
export function filterByCharacter(
  generations: Generation[],
  character: string,
): Generation[] {
  return character
    ? generations.filter((generation) => generation.character === character)
    : generations;
}

/** Character names in the history, in order of appearance. */
export function historyCharacters(generations: Generation[]): string[] {
  const names = generations.map((generation) => generation.character ?? "");
  return [...new Set(names.filter(Boolean))];
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
