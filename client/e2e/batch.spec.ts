import { expect, test } from "@playwright/test";

import { generation, mockBackend, savedReference } from "./mock";

test("a batch voices each line and links to the character's history", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    references: [
      savedReference(1, "あかり", 10),
      savedReference(2, "ずんだ", 11),
    ],
  });
  await page.goto("/batch");
  const character = page.getByLabel("キャラクター");
  await expect(character.locator("option")).toHaveText([
    "あかり（お手本 1件）",
    "ずんだ（お手本 1件）",
  ]);
  await character.selectOption("ずんだ");

  const start = page.getByRole("button", { name: "まとめて生成する" });
  await start.click();
  await expect(page.getByRole("alert")).toHaveText(
    "台詞を1行以上入力してください。",
  );
  expect(backend.batchRequests).toHaveLength(0);

  await page.getByLabel("声の作り方").selectOption("both");
  await page.getByLabel("声・話し方の説明").fill("明るく元気に話す。");
  await page
    .getByLabel("読み上げる台詞（1行に1本）")
    .fill("おはよう。\n\n失敗する台詞\nおやすみ。");
  await expect(page.getByText("3行（100行まで）")).toBeVisible();
  await start.click();

  const status = page.locator(".progress [role=status]");
  await expect(status).toHaveText(/生成しています… \d \/ 3/);
  await expect(start).toBeDisabled();
  await expect(
    page.getByRole("heading", { name: "ずんだ のまとめて生成" }),
  ).toBeFocused();
  await expect(status).toHaveText("完了しました（成功 2件・失敗 1件）", {
    timeout: 15000,
  });
  await expect(page.locator(".progress .error-banner")).toContainText(
    "失敗する台詞：生成できませんでした。",
  );
  expect(backend.batchRequests[0]).toMatchObject({
    character: "ずんだ",
    lines: "おはよう。\n\n失敗する台詞\nおやすみ。",
    settings: { mode: "both", caption: "明るく元気に話す。", num_steps: 40 },
  });

  await page
    .getByRole("link", { name: "生成履歴でずんだの音声を見る" })
    .click();
  await expect(page).toHaveURL(
    "/history?character=%E3%81%9A%E3%82%93%E3%81%A0",
  );
  await expect(page.getByLabel("キャラクター")).toHaveValue("ずんだ");
  await expect(page.locator(".card .ident")).toHaveText([
    /^ずんだ #\d+$/,
    /^ずんだ #\d+$/,
  ]);
});

test("returning to the batch page shows the running batch", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    references: [savedReference(1, "あかり", 10)],
  });
  backend.batches.push({
    id: "batch-9",
    character: "あかり",
    total: 2,
    done: 0,
    generation_ids: [],
    failed: [],
    finished: false,
    pending: ["一行目", "二行目"],
  });
  await page.goto("/batch");
  await expect(page.locator(".progress h2")).toHaveText(
    "あかり のまとめて生成",
  );
  await expect(page.locator(".progress [role=status]")).toHaveText(
    "完了しました（成功 2件・失敗 0件）",
    { timeout: 15000 },
  );
});

test("without references the batch page points to the history", async ({
  page,
}) => {
  await mockBackend(page, { generations: [generation(1, "声")] });
  await page.goto("/batch");
  await expect(page.getByText("まだお手本がありません。")).toBeVisible();
  await page.getByRole("link", { name: "生成履歴へ" }).click();
  await expect(page).toHaveURL("/history");
});

test("history shows character and ID, filters and keeps ★ after reload", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    generations: [
      generation(3, "ずんだの声", { character: "ずんだ" }),
      generation(2, "あかりの声", { character: "あかり" }),
      generation(1, "名前なし"),
    ],
  });
  await page.goto("/history");
  await expect(page.locator(".card .ident")).toHaveText([
    "ずんだ #3",
    "あかり #2",
    "#1",
  ]);
  const cleanup = page.getByRole("button", {
    name: /★もお手本もない音声を一括削除/,
  });
  await expect(cleanup).toContainText("（3件）");

  const star = page.getByRole("button", {
    name: "ずんだ #3をお気に入り（★）にする",
  });
  await expect(star).toHaveAttribute("aria-pressed", "false");
  await star.click();
  await expect(star).toHaveAttribute("aria-pressed", "true");
  await expect(cleanup).toContainText("（2件）");
  expect(backend.generations[0].favorite).toBe(true);

  await page.getByLabel("キャラクター").selectOption("あかり");
  await expect(page).toHaveURL(
    "/history?character=%E3%81%82%E3%81%8B%E3%82%8A",
  );
  await expect(page.locator(".card .ident")).toHaveText(["あかり #2"]);
  await expect(cleanup).toContainText("（2件）");

  await page.reload();
  await expect(page.locator(".card .ident")).toHaveText(["あかり #2"]);
  await page.getByLabel("キャラクター").selectOption("");
  await expect(
    page.getByRole("button", { name: "ずんだ #3をお気に入り（★）にする" }),
  ).toHaveAttribute("aria-pressed", "true");
});

test("a filter with no audio offers to clear it", async ({ page }) => {
  await mockBackend(page, { generations: [generation(1, "声")] });
  await page.goto("/history?character=%E3%81%A0%E3%82%8C");
  await expect(page.getByText("だれの音声はありません。")).toBeVisible();
  await page.getByRole("button", { name: "絞り込みを解除" }).click();
  await expect(page.locator(".card")).toHaveCount(1);
  await expect(page).toHaveURL("/history");
});
