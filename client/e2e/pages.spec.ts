import { expect, test } from "@playwright/test";

import { generation, mockBackend } from "./mock";

test("dictionary entries are registered, edited and deleted", async ({
  page,
}) => {
  const backend = await mockBackend(page);
  await page.goto("/dictionary");
  await expect(page.getByText("まだ登録はありません。")).toBeVisible();

  await page.getByRole("button", { name: "Irodori → いろどり" }).click();
  await page.getByRole("button", { name: "登録・更新" }).click();
  await expect(page.getByRole("status")).toHaveText("読み方を登録しました。");
  const rows = page.locator("tbody tr");
  await expect(rows).toHaveText([/Irodori\s*いろどり/]);

  await page.getByRole("button", { name: "Irodoriを編集" }).click();
  await expect(page.getByLabel("表記")).toHaveValue("Irodori");
  await expect(page.getByLabel("表記")).toBeFocused();
  await page.getByLabel("読み方").fill("イロドリ");
  await page.getByRole("button", { name: "登録・更新" }).click();
  await expect(page.getByRole("status")).toHaveText("読み方を更新しました。");
  await expect(rows).toHaveText([/Irodori\s*イロドリ/]);

  await page.getByLabel("読み方").fill("");
  await page.getByRole("button", { name: "登録・更新" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "「表記」と「読み方」の両方を入力してください。",
  );

  const remove = page.getByRole("button", { name: "Irodoriを削除" });
  await remove.click();
  const dialog = page.getByRole("dialog", { name: "読み方の削除" });
  await dialog.getByRole("button", { name: "キャンセル" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(remove).toBeFocused();
  await remove.click();
  await dialog.getByRole("button", { name: "削除する" }).click();
  await expect(page.getByText("まだ登録はありません。")).toBeVisible();
  expect(backend.dictionary).toEqual([]);
});

test("history lists, plays, downloads and deletes generations", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    generations: [
      generation(2, "新しい音声", { speed: 1.25, mode: "clone" }),
      generation(1, "古い音声"),
    ],
    fileErrors: [
      {
        path: "audio/old.wav",
        error: "Permission denied",
        created_at: "2026-09-26T00:00:00Z",
      },
    ],
  });
  await page.goto("/history");
  const cards = page.locator(".card");
  await expect(cards).toHaveCount(2);
  await expect(cards.first()).toContainText("新しい音声");
  await expect(cards.first()).toContainText(
    "お手本の声に似せる（ボイスクローン）",
  );
  await expect(cards.first()).toContainText("話速 1.25倍");
  await expect(page.getByRole("alert")).toContainText("audio/old.wav");
  await expect(cards.first().locator("audio")).toHaveAttribute(
    "src",
    "/api/generations/2/audio",
  );
  await expect(
    cards.first().getByRole("link", { name: /ダウンロード/ }),
  ).toHaveAttribute("href", "/api/generations/2/audio?download=1");

  await cards.first().getByRole("button", { name: /削除/ }).click();
  const dialog = page.getByRole("dialog", { name: "音声の削除" });
  await expect(dialog).toContainText("「新しい音声」");
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await cards.first().getByRole("button", { name: /削除/ }).click();
  await dialog.getByRole("button", { name: "削除する" }).click();
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText("古い音声");
  expect(backend.generations.map((item) => item.id)).toEqual([1]);
});

test("history shows empty and failed states", async ({ page }) => {
  await mockBackend(page);
  await page.goto("/history");
  await expect(page.locator('[data-state="empty"]')).toContainText(
    "まだ生成した音声はありません。",
  );

  await page.route("**/api/generations", (route) =>
    route.fulfill({ status: 500, json: { error: "boom" } }),
  );
  await page.reload();
  await expect(page.locator('[data-state="error"]')).toContainText(
    "生成履歴を読み込めませんでした。",
  );
});

test("credits keep the Gradio version's attributions", async ({ page }) => {
  await mockBackend(page);
  await page.goto("/credits");
  await expect(
    page.getByRole("link", { name: "Aratako / Chihiro Arata" }),
  ).toHaveAttribute("href", "https://github.com/Aratako/Irodori-TTS");
  await expect(page.getByText(/制作：ゆうぷろ/)).toBeVisible();
  await expect(
    page.getByRole("link", { name: "phasefield-audio" }),
  ).toBeVisible();
  await expect(
    page.getByText("本人の同意なしに声を複製して公開したり"),
  ).toBeVisible();
});
