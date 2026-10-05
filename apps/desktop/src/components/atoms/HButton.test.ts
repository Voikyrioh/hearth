import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HButton from "./HButton.vue";

describe("HButton", () => {
  it("emits click when active", async () => {
    const wrapper = mount(HButton, { slots: { default: "Valider" } });
    await wrapper.trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(1);
    expect(wrapper.attributes("aria-disabled")).toBeUndefined();
  });

  it("stays focusable, explains itself and ignores clicks when disabled", async () => {
    const wrapper = mount(HButton, {
      props: { disabled: true, hint: "Bientôt disponible" },
      slots: { default: "Valider" },
    });
    await wrapper.trigger("click");
    expect(wrapper.emitted("click")).toBeUndefined();
    expect(wrapper.attributes("aria-disabled")).toBe("true");
    expect(wrapper.attributes("title")).toBe("Bientôt disponible");
    expect(wrapper.attributes("disabled")).toBeUndefined();
  });

  it("applies the variant class", () => {
    const wrapper = mount(HButton, { props: { variant: "secondary" } });
    expect(wrapper.classes()).toContain("btn--secondary");
  });
});
