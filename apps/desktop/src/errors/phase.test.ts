import { describe, expect, it } from "vitest";
import { ErrorCodes } from "vue";
import { isRenderError } from "./phase";

const PROD = "https://vuejs.org/error-reference/#runtime-";

describe("isRenderError (both forms of Vue's error info)", () => {
  it("production form: the address ends with the error code", () => {
    for (const code of [
      ErrorCodes.SETUP_FUNCTION,
      ErrorCodes.RENDER_FUNCTION,
      ErrorCodes.COMPONENT_UPDATE,
      "m",
      "bm",
      "u",
    ]) {
      expect(isRenderError(`${PROD}${code}`), String(code)).toBe(true);
    }
    for (const code of [
      ErrorCodes.NATIVE_EVENT_HANDLER,
      ErrorCodes.COMPONENT_EVENT_HANDLER,
      ErrorCodes.SCHEDULER,
      ErrorCodes.APP_ERROR_HANDLER,
      ErrorCodes.ASYNC_COMPONENT_LOADER,
      3, // watcher callback
      "ec", // errorCaptured hook
    ]) {
      expect(isRenderError(`${PROD}${code}`), String(code)).toBe(false);
    }
  });

  it("development form: the label of the code, read from Vue's own table", () => {
    const labels = {
      0: "setup function",
      1: "render function",
      5: "native event handler",
      m: "mounted hook",
    };
    expect(isRenderError("setup function", labels)).toBe(true);
    expect(isRenderError("render function", labels)).toBe(true);
    expect(isRenderError("mounted hook", labels)).toBe(true);
    expect(isRenderError("native event handler", labels)).toBe(false);
    expect(isRenderError("n'importe quoi", labels)).toBe(false);
  });

  it("works with the real table of the Vue build under test (development labels)", () => {
    expect(isRenderError("render function")).toBe(true);
    expect(isRenderError("setup function")).toBe(true);
    expect(isRenderError("native event handler")).toBe(false);
    expect(isRenderError("component event handler")).toBe(false);
  });
});
