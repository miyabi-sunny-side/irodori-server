import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";

import App from "./App.svelte";

const INFO = {
  models: [
    { id: "Aratako/Irodori-TTS-v4.1-Small", label: "ベース（標準モデル）" },
  ],
  modes: [{ id: "design", label: "言葉で声を作る（ボイスデザイン）" }],
  devices: ["cpu"],
  precisions: { cpu: ["fp32"] },
  max_candidates: 32,
  emoji_groups: [],
};

describe("App", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    window.history.replaceState(null, "", "/");
  });

  it("keeps the invariant header and restores a deep page URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockImplementation((input) => {
        const payload =
          String(input) === "/api/info"
            ? INFO
            : { generations: [], file_errors: [] };
        return Promise.resolve(
          new Response(JSON.stringify(payload), { status: 200 }),
        );
      }),
    );
    window.history.replaceState(null, "", "/history");

    render(App);

    const header = screen.getByRole("banner");
    const title = header.querySelector('a[href="/"]');
    expect(title?.textContent).toContain("irodori-server");
    expect(header.querySelectorAll("a, button")).toHaveLength(2);

    const subHeader = screen.getByRole("heading", { level: 1 });
    expect(subHeader.textContent).toBe("生成履歴");
    expect(subHeader.querySelectorAll("a, button")).toHaveLength(0);
    await waitFor(() =>
      expect(screen.getByText("まだ生成した音声はありません。")).toBeTruthy(),
    );

    await fireEvent.click(title as HTMLElement);
    expect(window.location.pathname).toBe("/");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "音声作成",
    );
    expect(screen.getByText("① モデル・声を選ぶ")).toBeTruthy();
  });
});
