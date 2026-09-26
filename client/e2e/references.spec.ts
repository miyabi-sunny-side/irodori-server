import { expect, test } from "@playwright/test";

import { generation, mockBackend, savedReference } from "./mock";

test("a history item is saved as a reference under a character name", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    generations: [generation(2, "新しい声"), generation(1, "古い声")],
    references: [savedReference(7, "あかり", 50)],
  });
  await page.goto("/history");
  const cleanup = page.getByRole("button", {
    name: /★もお手本もない音声を一括削除/,
  });
  await expect(cleanup).toContainText("（2件）");
  const card = page.locator(".card").first();
  await card
    .getByRole("button", { name: "「新しい声」をお手本に保存" })
    .click();

  const dialog = page.getByRole("dialog", { name: "お手本に保存" });
  const name = dialog.getByLabel("キャラクター名");
  await expect(name).toBeFocused();
  await expect(dialog.locator("datalist option")).toHaveAttribute(
    "value",
    "あかり",
  );
  await dialog.getByRole("button", { name: "保存する" }).click();
  await expect(dialog.getByRole("alert")).toHaveText(
    "キャラクター名は1行・50文字までで入力してください。",
  );
  await expect(name).toHaveAttribute("aria-invalid", "true");

  await name.fill("  ずんだ ");
  await dialog.getByRole("button", { name: "保存する" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("status")).toHaveText(
    "「ずんだ」のお手本に保存しました。",
  );
  await expect(card.getByText("お手本に保存済み")).toBeFocused();
  await expect(cleanup).toContainText("（1件）");
  await expect(card.getByRole("button", { name: /お手本に保存/ })).toHaveCount(
    0,
  );
  expect(
    backend.references.map((r) => [r.character, r.source_generation_id]),
  ).toEqual([
    ["あかり", 50],
    ["ずんだ", 2],
  ]);

  await page.reload();
  await expect(page.locator(".card").first()).toContainText("お手本に保存済み");
});

test("bulk deletion removes only unsaved history after confirmation", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    generations: [
      generation(4, "星の声", { favorite: true }),
      generation(3, "残す声", { saved: true }),
      generation(2, "消す声 1"),
      generation(1, "消す声 2"),
    ],
  });
  await page.goto("/history");
  const cleanup = page.getByRole("button", {
    name: "★もお手本もない音声を一括削除（2件）",
  });
  await cleanup.click();
  const dialog = page.getByRole("dialog", {
    name: "★もお手本もない音声の一括削除",
  });
  await expect(dialog).toContainText("★もお手本への保存もない2件の音声");
  await dialog.getByRole("button", { name: "キャンセル" }).click();
  await expect(cleanup).toBeFocused();
  expect(backend.generations).toHaveLength(4);

  await cleanup.click();
  await dialog.getByRole("button", { name: "削除する" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "2件の音声を削除しました。",
  );
  await expect(page.locator(".card")).toHaveCount(2);
  await expect(page.locator(".card").nth(0)).toContainText("星の声");
  await expect(page.locator(".card").nth(1)).toContainText("残す声");
  await expect(
    page.getByRole("button", { name: "★もお手本もない音声を一括削除（0件）" }),
  ).toBeDisabled();
});

test("references are listed by character, played and deleted", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    references: [
      savedReference(1, "あかり", 11),
      savedReference(2, "あかり", 12),
      savedReference(3, "ずんだ", 13),
    ],
  });
  await page.goto("/references");
  const groups = page.locator("section.group");
  await expect(groups).toHaveCount(2);
  await expect(groups.first().getByRole("heading")).toHaveText(/あかり\s*2件/);
  await expect(groups.first()).toContainText("元の生成 #11");
  await expect(groups.first().locator("audio").first()).toHaveAttribute(
    "src",
    "/api/references/1/audio",
  );

  await page.getByRole("button", { name: "ずんだ 生成 13を削除" }).click();
  const dialog = page.getByRole("dialog", { name: "お手本の削除" });
  await expect(dialog).toContainText("「ずんだ」のお手本「生成 13」");
  await dialog.getByRole("button", { name: "削除する" }).click();
  await expect(groups).toHaveCount(1);
  expect(backend.references.map((r) => r.id)).toEqual([1, 2]);

  await page.goto("/");
  await page.getByRole("button", { name: "メニュー" }).click();
  await page.getByRole("link", { name: "お手本" }).click();
  await expect(page).toHaveURL(/\/references$/);
});

test("references page shows the empty state with a way to the history", async ({
  page,
}) => {
  await mockBackend(page);
  await page.goto("/references");
  await expect(page.locator('[data-state="empty"]')).toContainText(
    "まだお手本はありません。",
  );
  await page.getByRole("link", { name: "生成履歴へ" }).click();
  await expect(page).toHaveURL(/\/history$/);
});

test("saved references can be picked for cloning", async ({ page }) => {
  const backend = await mockBackend(page, {
    references: [
      savedReference(7, "あかり", 50),
      savedReference(8, "ずんだ", 51),
    ],
  });
  await page.goto("/");
  await page.getByLabel("声の作り方").selectOption("clone");
  const picker = page.getByLabel("保存したお手本から追加");
  await picker.selectOption({ label: "生成 51" });
  await picker.selectOption({ label: "生成 50" });
  await picker.selectOption({ label: "生成 51" });
  await expect(picker).toHaveValue("");
  await expect(page.locator(".reference-name")).toHaveText([
    "ずんだ / 生成 51",
    "あかり / 生成 50",
  ]);
  await page
    .getByRole("button", { name: "ずんだ / 生成 51を下に移動" })
    .click();

  await page.getByLabel("読み上げる文章").fill("お手本で話します。");
  await page.getByRole("button", { name: "音声を生成する" }).click();
  await expect(page.locator("audio.player")).toBeVisible();
  expect(backend.requests.at(-1)?.reference_ids).toEqual([7, 8]);
});
