import { waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";

import { initRouter, navigate, router } from "./router.svelte";
import { matchRoute } from "./routes";

describe("matchRoute", () => {
  it("maps / to the home route", () => {
    expect(matchRoute("/")).toEqual({ index: 0, params: {} });
  });

  it("maps each page URL to its route", () => {
    expect(matchRoute("/dictionary").index).toBe(1);
    expect(matchRoute("/history").index).toBe(2);
    expect(matchRoute("/credits").index).toBe(3);
    expect(matchRoute("/references").index).toBe(4);
    expect(matchRoute("/batch").index).toBe(5);
  });

  it("normalizes unknown paths to home", () => {
    expect(matchRoute("/no/such/page")).toEqual({ index: 0, params: {} });
  });
});

describe("router", () => {
  afterEach(() => {
    window.history.replaceState(null, "", "/");
  });

  it("navigate() pushes history and updates the route state", () => {
    const teardown = initRouter();

    navigate("/dictionary");

    expect(window.location.pathname).toBe("/dictionary");
    expect(router.index).toBe(1);
    teardown();
  });

  it("the browser back button returns to the previous route", async () => {
    const teardown = initRouter();
    window.history.replaceState(null, "", "/");
    navigate("/dictionary");

    window.history.back();

    await waitFor(() => expect(router.index).toBe(0));
    expect(window.location.pathname).toBe("/");
    teardown();
  });

  it("intercepts clicks on internal links", () => {
    const teardown = initRouter();
    const anchor = document.createElement("a");
    anchor.href = "/history";
    anchor.textContent = "card";
    document.body.appendChild(anchor);

    anchor.click();

    expect(window.location.pathname).toBe("/history");
    expect(router.index).toBe(2);
    anchor.remove();
    teardown();
  });

  it("leaves download links to the browser", () => {
    const teardown = initRouter();
    const anchor = document.createElement("a");
    anchor.href = "/api/generations/1/audio?download=1";
    anchor.setAttribute("download", "");
    document.body.appendChild(anchor);
    let prevented = true;
    // Registered after the router, so it observes the router's decision.
    const observe = (event: MouseEvent) => {
      prevented = event.defaultPrevented;
      event.preventDefault(); // jsdom cannot download; keep the page
    };
    document.addEventListener("click", observe);

    anchor.click();

    expect(prevented).toBe(false);
    expect(window.location.pathname).toBe("/");
    document.removeEventListener("click", observe);
    anchor.remove();
    teardown();
  });
});
