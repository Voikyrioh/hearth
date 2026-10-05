import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createApp, defineComponent, h } from "vue";
import { useToastsStore } from "@/stores/toasts";
import { freshBridge } from "@/test/app";
import { installErrorHandlers } from "./install";
import { MAX_REPORT_CHARS, MAX_REPORTS, reportUiError, resetReportRate } from "./report";

type Call = { cmd: string; args?: Record<string, string> };

function spyIpc() {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: args as Record<string, string> });
  });
  return calls;
}

beforeEach(() => {
  freshBridge();
  resetReportRate();
});
afterEach(() => clearMocks());

describe("reportUiError", () => {
  it("shows a discreet notification and writes to the client log through the typed command", async () => {
    const calls = spyIpc();
    reportUiError(new Error("boum"), "test");
    await flushPromises();
    expect(useToastsStore().items.map((t) => t.kind)).toEqual(["error"]);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.cmd).toBe("log_frontend_error");
    expect(calls[0]?.args?.source).toBe("test");
    expect(calls[0]?.args?.message).toContain("boum");
  });

  it("bounds the size of what it sends", async () => {
    const calls = spyIpc();
    reportUiError("x".repeat(MAX_REPORT_CHARS * 3), "test");
    await flushPromises();
    expect(calls[0]?.args?.message).toHaveLength(MAX_REPORT_CHARS);
  });

  it("bounds the rate: a loop of errors makes a counter, and few log lines", async () => {
    const calls = spyIpc();
    for (let i = 0; i < 100; i++) reportUiError(new Error("encore"), "loop");
    await flushPromises();
    expect(calls).toHaveLength(MAX_REPORTS);
    const toasts = useToastsStore();
    expect(toasts.items).toHaveLength(1);
    expect(toasts.items[0]?.count).toBe(100);
  });

  it("never throws, even without any Tauri bridge (browser) or with odd values", () => {
    expect(() => reportUiError(new Error("x"), "browser")).not.toThrow();
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(() => reportUiError(circular, "odd")).not.toThrow();
    expect(() => reportUiError(undefined, "odd")).not.toThrow();
  });
});

describe("installErrorHandlers", () => {
  it("catches a component error that nothing else caught: notification, log, no blank screen", async () => {
    const calls = spyIpc();
    const el = document.createElement("div");
    document.body.appendChild(el);
    const Boom = defineComponent({
      setup() {
        throw new Error("rendu cassé");
      },
      render: () => h("p"),
    });
    const app = createApp(Boom);
    app.use(freshBridge().pinia);
    installErrorHandlers(app, window);
    vi.spyOn(console, "warn").mockImplementation(() => {});
    app.mount(el);
    await flushPromises();
    expect(useToastsStore().items).toHaveLength(1);
    expect(calls[0]?.args?.source).toContain("vue:");
    expect(calls[0]?.args?.message).toContain("rendu cassé");
    app.unmount();
  });

  it("catches unhandled promise rejections and script errors", async () => {
    const calls = spyIpc();
    const app = createApp(defineComponent({ render: () => h("div") }));
    const target = new EventTarget() as unknown as Window;
    installErrorHandlers(app, target);
    const rejection = new Event("unhandledrejection");
    Object.assign(rejection, { reason: new Error("promesse") });
    target.dispatchEvent(rejection);
    const failure = new Event("error");
    Object.assign(failure, { error: new Error("script"), message: "script" });
    target.dispatchEvent(failure);
    await flushPromises();
    expect(calls.map((c) => c.args?.source)).toEqual(["unhandledrejection", "window"]);
    expect(useToastsStore().items[0]?.count).toBe(2);
  });

  it("mounts without a blank screen when a page crashes under the app", async () => {
    const wrapper = mount(defineComponent({ render: () => h("p", "ok") }), {
      global: { plugins: [freshBridge().pinia] },
    });
    expect(wrapper.text()).toBe("ok");
  });
});
