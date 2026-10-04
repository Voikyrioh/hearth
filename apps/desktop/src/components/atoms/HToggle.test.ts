import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HToggle from "./HToggle.vue";

describe("HToggle", () => {
  it("exposes a switch role with its state", () => {
    const wrapper = mount(HToggle, { props: { modelValue: true } });
    expect(wrapper.attributes("role")).toBe("switch");
    expect(wrapper.attributes("aria-checked")).toBe("true");
  });

  it("emits the opposite value on click", async () => {
    const wrapper = mount(HToggle, { props: { modelValue: false } });
    await wrapper.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toEqual([[true]]);
  });

  it("follows the HButton convention when disabled: focusable, explained, inert", async () => {
    const wrapper = mount(HToggle, {
      props: { modelValue: false, disabled: true, hint: "Pas encore" },
    });
    await wrapper.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(wrapper.attributes("aria-disabled")).toBe("true");
    expect(wrapper.attributes("title")).toBe("Pas encore");
    expect(wrapper.attributes("disabled")).toBeUndefined();
  });

  it("ignores a second click while busy and says so", async () => {
    const wrapper = mount(HToggle, { props: { modelValue: false, busy: true } });
    await wrapper.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(wrapper.attributes("aria-busy")).toBe("true");
    expect(wrapper.attributes("aria-disabled")).toBe("true");
  });
});
