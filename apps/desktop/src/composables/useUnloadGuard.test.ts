import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent, h } from "vue";
import { useUnloadGuard } from "./useUnloadGuard";

// Revue UX 2026-10-24, constat 7 : un rechargement avec une saisie en cours pose la question native du navigateur.

function ask(): BeforeUnloadEvent {
  const event = new Event("beforeunload", { cancelable: true }) as BeforeUnloadEvent;
  window.dispatchEvent(event);
  return event;
}

function host(dirty: () => boolean) {
  return mount(
    defineComponent({
      setup() {
        useUnloadGuard(dirty);
        return () => h("div");
      },
    }),
  );
}

describe("useUnloadGuard", () => {
  it("demande confirmation seulement quand une saisie existe, relue à chaque tentative", () => {
    let dirty = false;
    const wrapper = host(() => dirty);
    expect(ask().defaultPrevented).toBe(false);
    dirty = true;
    expect(ask().defaultPrevented).toBe(true);
    dirty = false;
    expect(ask().defaultPrevented).toBe(false);
    wrapper.unmount();
  });

  it("ne garde plus rien une fois la page partie", () => {
    const wrapper = host(() => true);
    wrapper.unmount();
    expect(ask().defaultPrevented).toBe(false);
  });
});
