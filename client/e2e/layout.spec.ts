import { expect, test } from "@playwright/test";

import { card, generation, mockBackend, voice } from "./mock";

// Set SHOTS=<dir> to keep full-page screenshots for visual review.
const shots = process.env.SHOTS;

const LONG_TEXT =
  "今日はとても良い天気ですね。長い文章でも画面の幅を超えずに折り返されることを確かめるための文章です。".repeat(
    3,
  );
const LONG_NAME = "とても長い話者の名前".repeat(4);

for (const viewport of [
  { width: 1440, height: 900 },
  { width: 390, height: 844 },
  { width: 320, height: 640 },
]) {
  for (const colorScheme of ["dark", "light"] as const) {
    test(`${viewport.width} ${colorScheme}: pages fit the width`, async ({
      page,
    }) => {
      await page.setViewportSize(viewport);
      await page.emulateMedia({ colorScheme });
      const backend = await mockBackend(page, {
        voices: [
          voice(1, "ひかり", { caption: "明るく、親しみやすい口調で話す。" }),
          voice(2, LONG_NAME, { model: "Aratako/Irodori-TTS-v4.1-Small" }),
        ],
        cards: [
          card("a", "おはようございます。", { voice_id: 1 }),
          card("b", LONG_TEXT, { voice_id: 2, speed: 1.25, silence: 1.5 }),
          card("c", "失敗する台詞", { voice_id: 1 }),
          card("d", ""),
        ],
        generations: [
          generation(2, LONG_TEXT, { speed: 1.25, saved: true }),
          generation(11, "星の声", { character: LONG_NAME, favorite: true }),
          ...Array.from({ length: 8 }, (_, i) =>
            generation(3 + i, `音声 ${i}`),
          ),
        ],
        dictionary: [
          { word: "Irodori", reading: "いろどり" },
          { word: "とても長い表記の例".repeat(4), reading: "よみ".repeat(20) },
        ],
      });

      await page.goto("/");
      const generate = page.getByRole("button", {
        name: "未生成・変更分を生成",
      });
      await expect(generate).toBeVisible();
      expect(
        await page.evaluate(
          () => getComputedStyle(document.body).backgroundColor,
        ),
      ).toBe(colorScheme === "dark" ? "rgb(25, 25, 25)" : "rgb(250, 246, 239)");
      expect(await page.locator("header").locator("a, button").count()).toBe(2);
      const box = await generate.boundingBox();
      expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height);

      // Generated, failed, long and empty cards with the settings open.
      await generate.click();
      await page
        .getByRole("dialog", { name: "長いセリフの生成" })
        .getByRole("button", { name: "このまま生成" })
        .click();
      await expect(page.getByRole("status").first()).toContainText(
        "生成が終わりました",
      );
      expect(backend.job?.failed).toBe(1);
      await page
        .getByRole("button", { name: "セリフ 1に絵文字を挿入" })
        .click();
      for (const summary of [
        "生成設定",
        "詳細設定（通常は変更不要）",
        "オプション",
        "入力済データ",
      ]) {
        await page.getByText(summary, { exact: true }).click();
      }

      for (const path of [
        "/",
        "/voices",
        "/dictionary",
        "/history",
        "/credits",
      ]) {
        if (path !== "/") {
          await page.goto(path);
          await expect(page.locator("main")).not.toBeEmpty();
          await page.waitForLoadState("networkidle");
        }
        const width = await page.evaluate(
          () => document.documentElement.scrollWidth,
        );
        expect(width, path).toBe(viewport.width);
        if (shots) {
          await page.evaluate(() => window.scrollTo(0, 0));
          const name = path === "/" ? "script" : path.slice(1);
          await page.screenshot({
            path: `${shots}/${name}-${viewport.width}-${colorScheme}.png`,
            fullPage: true,
          });
        }
      }
    });
  }
}
