import { expect, test } from "@playwright/test";

import { generation, mockBackend } from "./mock";

// Set SHOTS=<dir> to keep full-page screenshots for visual review.
const shots = process.env.SHOTS;

const LONG_TEXT =
  "今日はとても良い天気ですね。長い文章でも画面の幅を超えずに折り返されることを確かめるための文章です。".repeat(
    3,
  );

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
      await mockBackend(page, {
        generations: [
          generation(2, LONG_TEXT, { speed: 1.25 }),
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
      const generate = page.getByRole("button", { name: "音声を生成する" });
      await expect(generate).toBeEnabled();
      expect(
        await page.evaluate(
          () => getComputedStyle(document.body).backgroundColor,
        ),
      ).toBe(colorScheme === "dark" ? "rgb(25, 25, 25)" : "rgb(250, 246, 239)");
      expect(await page.locator("header").locator("a, button").count()).toBe(2);
      if (shots) {
        await page.screenshot({
          path: `${shots}/first-view-${viewport.width}-${colorScheme}.png`,
        });
      }
      if (viewport.width === 1440) {
        const box = await generate.boundingBox();
        expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height);
      }

      await page.getByLabel("読み上げる文章").fill(LONG_TEXT);
      await page.getByText("生成設定", { exact: true }).click();
      await page.getByRole("spinbutton", { name: "生成する候補数" }).fill("2");
      await page.getByRole("spinbutton", { name: "生成する候補数" }).blur();
      await generate.click();
      await expect(page.locator("audio.player")).toBeVisible();
      await page.getByText("詳細設定（通常は変更不要）").click();

      for (const path of ["/", "/dictionary", "/history", "/credits"]) {
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
          const name = path === "/" ? "create" : path.slice(1);
          await page.screenshot({
            path: `${shots}/${name}-${viewport.width}-${colorScheme}.png`,
            fullPage: true,
          });
        }
      }
    });
  }
}
