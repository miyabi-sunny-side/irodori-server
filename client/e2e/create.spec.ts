import { expect, test } from "@playwright/test";

import { mockBackend } from "./mock";

test("generates, compares candidates, plays and offers the WAV", async ({
  page,
}) => {
  const backend = await mockBackend(page, { holdGenerate: true });
  await page.goto("/");
  await page.getByLabel("読み上げる文章").fill("こんにちは。");
  await page.getByText("生成設定", { exact: true }).click();
  await page.getByRole("spinbutton", { name: "生成する候補数" }).fill("2");
  await page.getByRole("spinbutton", { name: "生成する候補数" }).blur();
  await page.getByRole("spinbutton", { name: "話速（倍）" }).fill("1.25");
  await page.getByRole("spinbutton", { name: "話速（倍）" }).blur();

  const generate = page.getByRole("button", { name: "音声を生成する" });
  await generate.click();
  const status = page.getByRole("status").first();
  await expect(status).toContainText("生成しています");
  await expect(generate).toBeDisabled();
  backend.release?.();
  await expect(status).toContainText("生成が完了しました");

  const sent = backend.requests[0];
  expect(sent).toMatchObject({
    text: "こんにちは。",
    speed: 1.25,
    num_candidates: 2,
    model_device: "cuda",
    model_precision: "fp32",
    mode: "design",
  });

  const audio = page.locator("audio.player");
  await expect(audio).toHaveAttribute("src", "/api/generations/100/audio");
  await page.getByLabel("聞き比べる音声").selectOption({ label: "音声 2" });
  await expect(audio).toHaveAttribute("src", "/api/generations/101/audio");
  const played = await audio.evaluate(async (element: HTMLAudioElement) => {
    element.muted = true;
    await element.play();
    return !element.paused;
  });
  expect(played).toBe(true);

  const download = page.getByRole("link", { name: "WAVをダウンロード" });
  await expect(download).toHaveAttribute(
    "href",
    "/api/generations/101/audio?download=1",
  );
  // The browser's download manager fetches this outside page.route, so the
  // saved name comes from the real server's Content-Disposition.
  await expect(download).toHaveAttribute("download", "");
  await expect(page).toHaveURL("/");

  await page.getByText("実行記録（問い合わせ用）").click();
  await expect(page.getByLabel("詳しい実行記録（一部英語）")).toHaveValue(
    /seed_used: 1/,
  );
});

test("the playback volume is remembered and applied to the player", async ({
  page,
}) => {
  await mockBackend(page);
  await page.goto("/");
  await page.getByLabel("読み上げる文章").fill("音量");
  await page.getByRole("button", { name: "音声を生成する" }).click();
  await expect(page.locator("audio.player")).toBeVisible();

  await page.getByRole("slider", { name: /再生音量/ }).fill("40");
  await expect(page.getByText("再生音量：40％")).toBeVisible();
  expect(
    await page
      .locator("audio.player")
      .evaluate((a: HTMLAudioElement) => a.volume),
  ).toBeCloseTo(0.4);

  await page.reload();
  await expect(page.getByText("再生音量：40％")).toBeVisible();
});

test("the voice mode shows only the inputs it uses", async ({ page }) => {
  await mockBackend(page);
  await page.goto("/");
  const mode = page.getByLabel("声の作り方");
  const caption = page.getByLabel("声・話し方の説明");
  const references = page.getByText("お手本の音声（同意を得た声）");

  await expect(caption).toBeVisible();
  await expect(references).toHaveCount(0);
  await mode.selectOption("clone");
  await expect(caption).toHaveCount(0);
  await expect(references).toBeVisible();
  await mode.selectOption("both");
  await expect(caption).toBeVisible();
  await expect(references).toBeVisible();
  await mode.selectOption("auto");
  await expect(caption).toHaveCount(0);
  await expect(references).toHaveCount(0);
});

test("caption examples fill the description", async ({ page }) => {
  await mockBackend(page);
  await page.goto("/");
  await page
    .getByLabel("説明の入力例")
    .selectOption("明るく元気な声で、楽しそうに話す女性。");
  await expect(page.getByLabel("声・話し方の説明")).toHaveValue(
    "明るく元気な声で、楽しそうに話す女性。",
  );
});

test("reference audio uploads, reorders and is sent in order", async ({
  page,
}) => {
  const backend = await mockBackend(page);
  await page.goto("/");
  await page.getByLabel("声の作り方").selectOption("clone");
  await page.locator('input[type="file"]').setInputFiles([
    { name: "first.wav", mimeType: "audio/wav", buffer: Buffer.from("a") },
    { name: "second.m4a", mimeType: "audio/mp4", buffer: Buffer.from("b") },
    { name: "third.mp3", mimeType: "audio/mpeg", buffer: Buffer.from("c") },
  ]);
  const names = page.locator(".reference-name");
  await expect(names).toHaveText(["first.wav", "second.m4a", "third.mp3"]);

  await page.getByRole("button", { name: "third.mp3を上に移動" }).click();
  await page.getByRole("button", { name: "first.wavを削除" }).click();
  await expect(names).toHaveText(["third.mp3", "second.m4a"]);
  await expect(
    page.getByRole("button", { name: "third.mp3を上に移動" }),
  ).toBeDisabled();

  await page.getByLabel("読み上げる文章").fill("似せて話します。");
  await page.getByRole("button", { name: "音声を生成する" }).click();
  await expect(page.getByRole("status").first()).toContainText(
    "生成が完了しました",
  );
  expect(backend.requests[0]).toMatchObject({
    mode: "clone",
    reference_ids: [3, 2],
  });
});

test("emoji insert at the caret and replace a selection", async ({ page }) => {
  await mockBackend(page);
  await page.goto("/");
  const text = page.getByLabel("読み上げる文章");
  await text.fill("こんにちは");
  await text.evaluate((area: HTMLTextAreaElement) =>
    area.setSelectionRange(2, 2),
  );
  await page.getByRole("button", { name: "楽しげの絵文字を挿入" }).click();
  await expect(text).toHaveValue("こん😊にちは");
  await expect(text).toBeFocused();
  expect(
    await text.evaluate((area: HTMLTextAreaElement) => area.selectionStart),
  ).toBe(4);

  await text.evaluate((area: HTMLTextAreaElement) =>
    area.setSelectionRange(0, 2),
  );
  await page.getByRole("tab", { name: "話し方・演出" }).click();
  await page.getByRole("button", { name: "囁きの絵文字を挿入" }).click();
  await expect(text).toHaveValue("👂😊にちは");
});

test("the dictionary preview reflects the checkbox", async ({ page }) => {
  await mockBackend(page, {
    dictionary: [{ word: "Irodori", reading: "いろどり" }],
  });
  await page.goto("/");
  await page.getByLabel("読み上げる文章").fill("Irodoriで話します。");
  const check = page.getByRole("button", {
    name: "読み辞書を反映した文章を確認",
  });
  const preview = page.getByLabel("読み上げに使う文章（確認用）");
  await expect(preview).toHaveCount(0);
  await check.click();
  await expect(preview).toHaveValue("いろどりで話します。");
  await page.getByLabel("読み辞書を使う").uncheck();
  await check.click();
  await expect(preview).toHaveValue("Irodoriで話します。");
});

test("the form survives a visit to the dictionary", async ({ page }) => {
  await mockBackend(page);
  await page.goto("/");
  await page.getByLabel("読み上げる文章").fill("残しておく文章");
  await page.getByRole("link", { name: "読み辞書を編集" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("読み辞書");
  await page.getByRole("link", { name: "irodori-server" }).click();
  await expect(page.getByLabel("読み上げる文章")).toHaveValue("残しておく文章");
});

test("sway settings unlock only for Sway, and unload reports", async ({
  page,
}) => {
  await mockBackend(page);
  await page.goto("/");
  await page.getByText("詳細設定（通常は変更不要）").click();
  const sway = page.getByRole("spinbutton", { name: "Swayの調整値" });
  await expect(sway).toBeDisabled();
  await page.getByLabel("計算の進め方").selectOption("sway");
  await expect(sway).toBeEnabled();

  await page.getByLabel("音声生成に使う機器").selectOption("cpu");
  await expect(page.getByLabel("音声生成の計算精度")).toHaveValue("fp32");
  await page.getByLabel("音声生成に使う機器").selectOption("cuda");
  await page.getByLabel("音声生成の計算精度").selectOption("bf16");
  await expect(page.getByLabel("音声生成の計算精度")).toHaveValue("bf16");

  await page.getByRole("button", { name: "モデルをメモリから解放" }).click();
  await expect(page.getByRole("status").first()).toContainText(
    "モデルをメモリから解放しました",
  );
});

test("a failed generation shows the reason and keeps the log", async ({
  page,
}) => {
  await mockBackend(page);
  await page.route("**/api/generate", (route) =>
    route.fulfill({
      status: 502,
      json: {
        error: "GPUメモリが不足しました。",
        log: "Traceback: CUDA out of memory",
      },
    }),
  );
  await page.goto("/");
  await page.getByLabel("読み上げる文章").fill("失敗");
  await page.getByRole("button", { name: "音声を生成する" }).click();
  const status = page.getByRole("status").first();
  await expect(status).toHaveText("GPUメモリが不足しました。");
  await expect(status).toHaveAttribute("data-kind", "error");
  await page.getByText("実行記録（問い合わせ用）").click();
  await expect(page.getByLabel("詳しい実行記録（一部英語）")).toHaveValue(
    /out of memory/,
  );
  await expect(
    page.getByRole("button", { name: "音声を生成する" }),
  ).toBeEnabled();
});
