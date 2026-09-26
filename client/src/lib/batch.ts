// Pure helpers for まとめて生成 (docs/api.md「まとめて生成」).

import type { Batch, Reference } from "./api";

/** Mirrors the server's MAX_BATCH_LINES. */
export const MAX_BATCH_LINES = 100;

/** Counts the lines the server will voice and explains an unusable input. */
export function lineSummary(raw: string): { count: number; error: string } {
  const count = raw.split("\n").filter((line) => line.trim() !== "").length;
  if (count === 0) {
    return { count, error: "台詞を1行以上入力してください。" };
  }
  if (count > MAX_BATCH_LINES) {
    return {
      count,
      error: `一度に生成できるのは${MAX_BATCH_LINES}行までです。`,
    };
  }
  return { count, error: "" };
}

export function progressText(batch: Batch): string {
  return batch.finished
    ? `完了しました（成功 ${batch.generation_ids.length}件・失敗 ${batch.failed.length}件）`
    : `生成しています… ${batch.done} / ${batch.total}`;
}

/** Characters that have saved references, with how many each has. */
export function characterOptions(
  references: Reference[],
): { name: string; count: number }[] {
  const options: { name: string; count: number }[] = [];
  for (const reference of references) {
    if (!reference.character) continue;
    const found = options.find((option) => option.name === reference.character);
    if (found) found.count += 1;
    else options.push({ name: reference.character, count: 1 });
  }
  return options;
}
