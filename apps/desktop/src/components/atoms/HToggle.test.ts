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

  it("does nothing when disabled", async () => {
    const wrapper = mount(HToggle, { props: { modelValue: false, disabled: true } });
    await wrapper.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
  });
});
