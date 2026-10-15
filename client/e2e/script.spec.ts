import { expect, test, type Page } from "@playwright/test";

import { card, mockBackend, voice } from "./mock";

const cardItem = (page: Page, n: number) =>
  page.getByRole("listitem", { name: `セリフ ${n}` });

test("cards are edited, saved, generated on the server and played", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    voices: [voice(1, "ひかり"), voice(2, "てがみ")],
  });
  await page.goto("/");
  const first = cardItem(page, 1);
  await first
    .getByLabel("セリフ 1の話者")
    .selectOption({ label: "ひかり（MF（高速））" });
  await first.getByLabel("セリフ 1の文章").fill("おはようございます。");
  await page.getByRole("button", { name: "セリフを追加" }).click();
  const second = cardItem(page, 2);
  await expect(second.getByLabel("セリフ 2の話者")).toHaveValue("1");
  await second.getByLabel("セリフ 2の話者").selectOption("2");
  await second.getByLabel("セリフ 2の文章").fill("よろしくお願いします。");
  await second.getByRole("spinbutton").first().fill("1.6");
  await second.getByRole("spinbutton").first().blur();
  await expect(second.getByRole("spinbutton").first()).toHaveValue("1.5");
  await expect(first.locator(".badge")).toHaveText("未作成");

  await expect
    .poll(
      () =>
        (backend.saves.at(-1) as { script: { cards: unknown[] } } | undefined)
          ?.script.cards,
    )
    .toMatchObject([
      { text: "おはようございます。", voice_id: 1 },
      { text: "よろしくお願いします。", voice_id: 2, speed: 1.5 },
    ]);

  await page.getByRole("button", { name: "未生成・変更分を生成" }).click();
  await expect(page.getByRole("button", { name: "停止" })).toBeVisible();
  await expect(page.getByRole("status").first()).toContainText(
    "生成が終わりました（2件）",
  );
  await expect(first.locator(".badge")).toHaveText("完了");
  await expect(second.locator("audio")).toHaveAttribute(
    "src",
    /\/api\/generations\/\d+\/audio/,
  );
  await expect(
    second.getByRole("link", { name: "セリフ 2のWAVをダウンロード" }),
  ).toHaveAttribute("download", "002_てがみ_20261015.wav");

  // Editing the text marks the card changed until it is generated again.
  await first.getByLabel("セリフ 1の文章").fill("こんばんは。");
  await expect(first.locator(".badge")).toHaveText("変更あり");
  await page.getByRole("button", { name: "連続再生" }).click();
  await expect(page.getByRole("status").first()).toContainText(
    "セリフ 1 に、現在の入力内容に対応する音声がありません",
  );
  await first.getByRole("button", { name: "セリフ 1を生成" }).click();
  await expect(first.locator(".badge")).toHaveText("完了");

  await page.getByRole("button", { name: "連続再生" }).click();
  await expect(
    page.getByRole("button", { name: "再生を止める" }),
  ).toBeVisible();
  await expect(page.locator("li.card.playing")).toHaveCount(1);
  await page.getByRole("button", { name: "再生を止める" }).click();
  await expect(page.locator("li.card.playing")).toHaveCount(0);

  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "個別にZIP" }).click();
  expect((await download).suggestedFilename()).toBe("まとめ音声.zip");
});

test("a failed card shows the reason and stopping waits for the running card", async ({
  page,
}) => {
  await mockBackend(page, {
    voices: [voice(1, "ひかり")],
    cards: [
      card("a", "一", { voice_id: 1 }),
      card("b", "失敗する", { voice_id: 1 }),
      card("c", "三", { voice_id: 1 }),
    ],
  });
  await page.goto("/");
  await page.getByRole("button", { name: "すべて生成" }).click();
  await expect(cardItem(page, 2).locator(".badge")).toHaveText("失敗");
  await expect(cardItem(page, 2)).toContainText("生成できませんでした。");
  await expect(page.getByRole("status").first()).toContainText("失敗 1件");

  await page.getByRole("button", { name: "すべて生成" }).click();
  await page.getByRole("button", { name: "停止" }).click();
  await expect(page.getByRole("button", { name: "すべて生成" })).toBeVisible();
});

test("cards are split, duplicated, moved, deleted and restored with undo", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    voices: [voice(1, "ひかり")],
    cards: [
      card("a", "今日はとても良い天気ですね、散歩に行きましょう。".repeat(10), {
        voice_id: 1,
        speed: 1.25,
      }),
      card("b", "二枚目", { voice_id: 1 }),
    ],
  });
  await page.goto("/");
  await expect(cardItem(page, 1).locator(".estimate")).toHaveClass(/long/);
  await cardItem(page, 1)
    .getByRole("button", { name: "セリフ 1を分割" })
    .click();
  await expect(page.getByRole("status").first()).toContainText(
    "続きのセリフを2枚追加しました。",
  );
  await expect(page.locator("li.card")).toHaveCount(4);
  await expect(cardItem(page, 3).getByRole("spinbutton").first()).toHaveValue(
    "1.25",
  );

  await cardItem(page, 4)
    .getByRole("button", { name: "セリフ 4を複製" })
    .click();
  await expect(cardItem(page, 5).getByLabel("セリフ 5の文章")).toHaveValue(
    "二枚目",
  );
  await cardItem(page, 5).getByLabel("セリフ 5の文章").fill("五枚目");
  const grip = cardItem(page, 5).getByRole("button", { name: /つかんで移動/ });
  await grip.focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(cardItem(page, 4).getByLabel("セリフ 4の文章")).toHaveValue(
    "五枚目",
  );
  await expect(
    cardItem(page, 4).getByRole("button", { name: /つかんで移動/ }),
  ).toBeFocused();

  await cardItem(page, 1)
    .getByRole("button", { name: "セリフ 1を削除" })
    .click();
  await expect(page.locator("li.card")).toHaveCount(4);
  await expect.poll(() => backend.cards.length).toBe(4);
  await page.getByRole("button", { name: "元に戻す" }).click();
  await expect(page.locator("li.card")).toHaveCount(5);
  await expect.poll(() => backend.cards.length).toBe(5);
  await page.getByRole("button", { name: "やり直す" }).click();
  await expect(page.locator("li.card")).toHaveCount(4);
});

test("emoji insert at the caret of the card", async ({ page }) => {
  await mockBackend(page, {
    voices: [voice(1, "ひかり")],
    cards: [card("a", "こんにちは", { voice_id: 1 })],
  });
  await page.goto("/");
  const text = page.getByLabel("セリフ 1の文章");
  await text.click();
  await text.evaluate((area: HTMLTextAreaElement) =>
    area.setSelectionRange(2, 2),
  );
  await page.getByRole("button", { name: "セリフ 1に絵文字を挿入" }).click();
  await page.getByRole("button", { name: "楽しげの絵文字を挿入" }).click();
  await expect(text).toHaveValue("こん😊にちは");
  await expect(text).toBeFocused();
});

test("another screen's edit is reported and the latest script reloads", async ({
  page,
}) => {
  const backend = await mockBackend(page, {
    voices: [voice(1, "ひかり")],
    cards: [card("a", "元の文章", { voice_id: 1 })],
  });
  await page.goto("/");
  backend.conflictNext = true;
  backend.cards = [card("a", "別の画面の文章", { voice_id: 1 })];
  await page.getByLabel("セリフ 1の文章").fill("この画面の文章");
  const banner = page.getByRole("alert");
  await expect(banner).toContainText("別の画面で台本が更新されました。");
  await banner.getByRole("button", { name: "最新の台本を読み込む" }).click();
  await expect(page.getByLabel("セリフ 1の文章")).toHaveValue("別の画面の文章");
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("long cards ask before generating", async ({ page }) => {
  const backend = await mockBackend(page, {
    voices: [voice(1, "ひかり")],
    cards: [card("a", "長".repeat(80), { voice_id: 1 })],
  });
  await page.goto("/");
  await page.getByRole("button", { name: "未生成・変更分を生成" }).click();
  const dialog = page.getByRole("dialog", { name: "長いセリフの生成" });
  await expect(dialog).toContainText("セリフ 1");
  await dialog.getByRole("button", { name: "キャンセル" }).click();
  expect(backend.job).toBeNull();
  await page.getByRole("button", { name: "未生成・変更分を生成" }).click();
  await dialog.getByRole("button", { name: "このまま生成" }).click();
  await expect(cardItem(page, 1).locator(".badge")).toHaveText("完了");
});

test("the script is saved to a file and read back", async ({ page }) => {
  const backend = await mockBackend(page, {
    voices: [voice(1, "ひかり")],
    cards: [card("a", "保存する文章", { voice_id: 1, silence: 2 })],
  });
  await page.goto("/");
  await page.getByText("入力済データ").click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "ファイルに保存" }).click();
  const file = await (await download).path();

  await page.getByRole("button", { name: "文章をすべて消す" }).click();
  await page
    .getByRole("dialog", { name: "文章をすべて消す" })
    .getByRole("button", { name: "消す" })
    .click();
  await expect(page.getByLabel("セリフ 1の文章")).toHaveValue("");

  await page.locator('input[type="file"]').setInputFiles(file);
  await expect(page.getByLabel("セリフ 1の文章")).toHaveValue("保存する文章");
  await expect.poll(() => backend.cards[0]?.silence).toBe(2);

  await page.locator('input[type="file"]').setInputFiles({
    name: "bad.json",
    mimeType: "application/json",
    buffer: Buffer.from("{}"),
  });
  await expect(page.getByRole("status").first()).toContainText(
    "入力済データを読み込めません",
  );
  await expect(page.getByLabel("セリフ 1の文章")).toHaveValue("保存する文章");
});

test("shared settings leave steps to the model and devices to the engine", async ({
  page,
}) => {
  const backend = await mockBackend(page, { voices: [voice(1, "ひかり")] });
  await page.goto("/");
  await page.getByText("生成設定", { exact: true }).click();
  await expect(page.getByLabel("生成の計算回数")).toHaveValue("");
  await page.getByLabel("生成の計算回数").fill("8");
  await page.getByLabel("生成の計算回数").blur();
  await expect
    .poll(
      () =>
        (
          backend.saves.at(-1) as {
            script: { settings: { num_steps: number } };
          }
        )?.script.settings.num_steps,
    )
    .toBe(8);
  await page.getByText("詳細設定（通常は変更不要）").click();
  await expect(page.getByLabel("音声生成に使う機器")).toHaveValue("");
  await expect(
    page.getByRole("spinbutton", { name: "Swayの調整値" }),
  ).toBeDisabled();
  await page.getByLabel("計算の進め方").selectOption("sway");
  await expect(
    page.getByRole("spinbutton", { name: "Swayの調整値" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "モデルをメモリから解放" }).click();
  await expect(page.getByRole("status").first()).toContainText(
    "モデルをメモリから解放しました",
  );
});

test("without voices the script points to the voice registration", async ({
  page,
}) => {
  await mockBackend(page);
  await page.goto("/");
  await page.getByRole("link", { name: "声の登録" }).click();
  await expect(page).toHaveURL("/voices");
});
