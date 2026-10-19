import { afterEach, describe, expect, it } from "vitest";
import { installInputModality } from "./inputModality";

// FIX:01M5A5VJ8G6Q6FDJV9AX6ECF41 : le cadre de focus ne se montre qu'au clavier.
describe("installInputModality", () => {
  let off: (() => void) | null = null;
  afterEach(() => {
    off?.();
    off = null;
    delete document.documentElement.dataset.input;
  });

  it("marque « pointer » après un clic et « keyboard » après une touche", () => {
    off = installInputModality(document);
    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    expect(document.documentElement.dataset.input).toBe("pointer");
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    expect(document.documentElement.dataset.input).toBe("keyboard");
  });

  it("ignore un modificateur seul", () => {
    off = installInputModality(document);
    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Shift", bubbles: true }));
    expect(document.documentElement.dataset.input).toBe("pointer");
  });

  it("ne marque plus rien une fois retiré", () => {
    installInputModality(document)();
    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    expect(document.documentElement.dataset.input).toBeUndefined();
  });
});
