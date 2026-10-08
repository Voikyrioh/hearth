import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";
import HMiddleText from "./HMiddleText.vue";

// happy-dom n'a pas de mise en page : la largeur du début est simulée (10 px par caractère, boîte de 100 px).
describe("HMiddleText", () => {
  const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollWidth");
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollWidth", {
      configurable: true,
      get() {
        return (this.textContent?.length ?? 0) * 10;
      },
    });
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get: () => 100,
    });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
  });
  afterEach(() => {
    if (original) Object.defineProperty(HTMLElement.prototype, "scrollWidth", original);
    vi.unstubAllGlobals();
  });

  it("recalcule la troncature quand le texte change (un texte court devient long)", async () => {
    const wrapper = mount(HMiddleText, { props: { text: "/mnt/a" } });
    expect(wrapper.get("[data-middle]").attributes("tabindex")).toBeUndefined();
    await wrapper.setProps({
      text: "/var/lib/docker/rootfs/overlayfs/516ea538beeb5e0443f9fa80d43d3f8c4c7b5fb99d71715882d9cb491826482b",
    });
    await nextTick();
    await nextTick();
    expect(wrapper.get("[data-middle]").attributes("tabindex")).toBe("0");
    await wrapper.setProps({ text: "/mnt/b" });
    await nextTick();
    await nextTick();
    expect(wrapper.get("[data-middle]").attributes("tabindex")).toBeUndefined();
  });

  it("le texte complet est dans le document pour un lecteur d'écran, sans rôle inventé ni libellé", () => {
    const full =
      "/var/lib/docker/rootfs/overlayfs/516ea538beeb5e0443f9fa80d43d3f8c4c7b5fb99d71715882d9cb491826482b";
    const wrapper = mount(HMiddleText, { props: { text: full } });
    const root = wrapper.get("[data-middle]");
    expect(root.attributes("role")).toBeUndefined();
    expect(root.attributes("aria-label")).toBeUndefined();
    const readable = Array.from(root.element.querySelectorAll("*"))
      .filter((el) => el.getAttribute("aria-hidden") !== "true")
      .map((el) => el.textContent)
      .join("");
    expect(readable).toBe(full);
  });
});
