import { expect, test } from "@playwright/test";

import { generation, mockBackend, voice } from "./mock";

const MF = "Aratako/Irodori-TTS-v4.1-Small-MF";

test("a new voice is designed from candidates and registered", async ({
  page,
}) => {
  const backend = await mockBackend(page);
  await page.goto("/voices");
  await expect(page.getByText("まだ登録した声はありません。")).toBeVisible();
  const register = page.getByRole("button", { name: "この音声を登録する" });
  await expect(register).toBeDisabled();

  await page.getByLabel("使用モデル").selectOption(MF);
  await page
    .getByLabel("説明の入力例から選ぶ")
    .selectOption({ label: "女性・明るい" });
  await page.getByRole("button", { name: "候補の音声を作成" }).click();
  await expect(page.locator('input[name="candidate"]')).toHaveCount(3);
  expect(backend.requests[0]).toMatchObject({
    model: MF,
    mode: "design",
    caption: "明るく親しみやすい女性の声。自然な高さで、はっきりと話す。",
    num_candidates: 3,
    num_steps: null,
  });
  expect(String(backend.requests[0].text)).toMatch(/^おはようございます。/);

  await page.locator('input[name="candidate"]').nth(1).check();
  await page
    .getByLabel("話し方の入力例から選ぶ")
    .selectOption({ label: "穏やかに" });
  await page.getByLabel("話者名").fill(" ");
  await register.click();
  await expect(page.getByRole("alert")).toContainText("話者名を1行");
  await page.getByLabel("話者名").fill("ひかり");
  await register.click();
  await expect(page.getByText("「ひかり」を登録しました。")).toBeVisible();
  expect(backend.voiceRequests[0]).toEqual({
    name: "ひかり",
    model: MF,
    caption: "穏やかで、やさしい口調で話す。",
    generation_id: 101,
  });
  await expect(page.locator(".list .card")).toHaveCount(1);
  await expect(page.locator(".list .card")).toContainText("MF（高速）");
});

test("an uploaded voice is tested and registered as it is", async ({
  page,
}) => {
  const backend = await mockBackend(page, { referenceSeconds: 0.5 });
  await page.goto("/voices");
  await page.getByLabel("手元の音声をそのまま使う").check();
  await page.locator('input[type="file"]').setInputFiles({
    name: "me.wav",
    mimeType: "audio/wav",
    buffer: Buffer.from("RIFF"),
  });
  await expect(page.getByText("me.wav")).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("1～120秒");
  await expect(
    page.getByRole("button", { name: "この音声を登録する" }),
  ).toBeDisabled();
  backend.referenceSeconds = 5;
  await page.locator('input[type="file"]').setInputFiles({
    name: "me.wav",
    mimeType: "audio/wav",
    buffer: Buffer.from("RIFF"),
  });
  await expect(page.getByText("10秒未満の短い音声")).toBeVisible();
  await page.getByText("テスト音声を作る（任意）").click();
  await page.getByRole("button", { name: "テスト音声を作成" }).click();
  await expect(page.getByLabel("テスト音声")).toBeVisible();
  expect(backend.requests[0]).toMatchObject({
    mode: "clone",
    reference_ids: [2],
    num_candidates: 1,
  });
  await page.getByLabel("話者名").fill("わたし");
  await page.getByRole("button", { name: "この音声を登録する" }).click();
  await expect(page.getByText("「わたし」を登録しました。")).toBeVisible();
  expect(backend.voiceRequests[0]).toMatchObject({
    name: "わたし",
    reference_id: 2,
  });
});

test("voices are renamed with conflicts reported and deleted after confirmation", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    voices: [voice(1, "あかり", { caption: "明るく" }), voice(2, "そら")],
  });
  await page.goto("/voices");
  await page.getByRole("button", { name: "あかりの名前を変更" }).click();
  await expect(page.getByLabel("新しい話者名")).toBeFocused();
  backend.voices[0].revision = 5;
  await page.getByLabel("新しい話者名").fill("あかね");
  await page.getByRole("button", { name: "変更", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("話者が更新されています");
  await page.getByRole("button", { name: "変更", exact: true }).click();
  await expect(
    page.getByText("名前を「あかね」に変更しました。"),
  ).toBeVisible();

  const remove = page.getByRole("button", { name: "そらを削除" });
  await remove.click();
  const dialog = page.getByRole("dialog", { name: "話者の削除" });
  await dialog.getByRole("button", { name: "キャンセル" }).click();
  await expect(remove).toBeFocused();
  await remove.click();
  await dialog.getByRole("button", { name: "削除する" }).click();
  await expect(page.locator(".list .card")).toHaveCount(1);
  expect(backend.voices.map((v) => v.name)).toEqual(["あかね"]);
});

test("a history item is registered as a voice with its model and style", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    generations: [
      generation(3, "クローン", { mode: "clone", caption: "無視" }),
      generation(2, "デザイン", {
        mode: "design",
        caption: "低い声",
        model: MF,
      }),
    ],
  });
  await page.goto("/history");
  await page.getByRole("button", { name: "「デザイン」を話者に登録" }).click();
  const dialog = page.getByRole("dialog", { name: "話者に登録" });
  await expect(dialog).toContainText("MF（高速）・低い声");
  await dialog.getByLabel("話者名").fill("ひくい");
  await dialog.getByRole("button", { name: "登録する" }).click();
  await expect(
    page.getByText("「ひくい」を話者に登録しました。"),
  ).toBeVisible();
  await expect(page.locator("#saved-2")).toBeFocused();
  expect(backend.voiceRequests[0]).toEqual({
    name: "ひくい",
    model: MF,
    caption: "低い声",
    generation_id: 2,
  });

  await page.getByRole("button", { name: "「クローン」を話者に登録" }).click();
  await dialog.getByLabel("話者名").fill("くろーん");
  await dialog.getByRole("button", { name: "登録する" }).click();
  expect(backend.voiceRequests[1]).toMatchObject({ caption: "" });
});

test("bulk deletion keeps ★, voices and script audio", async ({ page }) => {
  const backend = await mockBackend(page, {
    generations: [
      generation(4, "台本", { scripted: true, character: "ひかり" }),
      generation(3, "星", { favorite: true }),
      generation(2, "登録済み", { saved: true }),
      generation(1, "消える"),
    ],
  });
  await page.goto("/history");
  await page.getByLabel("話者", { exact: true }).selectOption("ひかり");
  await expect(page.locator(".card")).toHaveCount(1);
  await expect(page).toHaveURL(/character=/);
  await page.getByLabel("話者", { exact: true }).selectOption("");
  const cleanup = page.getByRole("button", {
    name: "使っていない音声を一括削除（1件）",
  });
  await cleanup.click();
  await page
    .getByRole("dialog", { name: "使っていない音声の一括削除" })
    .getByRole("button", { name: "削除する" })
    .click();
  await expect(page.getByText("1件の音声を削除しました。")).toBeVisible();
  expect(backend.generations.map((g) => g.id)).toEqual([4, 3, 2]);
});
