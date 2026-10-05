import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HToggle from "./HToggle.vue";

describe("HToggle", () => {
  it("exposes a switch role with its state", () => {
    const button = mount(HToggle, { props: { modelValue: true } }).get("button");
    expect(button.attributes("role")).toBe("switch");
    expect(button.attributes("aria-checked")).toBe("true");
  });

  it("emits the opposite value on click", async () => {
    const wrapper = mount(HToggle, { props: { modelValue: false } });
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("update:modelValue")).toEqual([[true]]);
  });

  it("follows the HButton convention when disabled: focusable, explained by a tooltip, inert", async () => {
    const wrapper = mount(HToggle, {
      props: { modelValue: false, disabled: true, hint: "Pas encore" },
    });
    const button = wrapper.get("button");
    await button.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe("Pas encore");
    expect(button.attributes("disabled")).toBeUndefined();
  });

  it("ignores a second click while busy and says so", async () => {
    const wrapper = mount(HToggle, { props: { modelValue: false, busy: true } });
    const button = wrapper.get("button");
    await button.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(button.attributes("aria-busy")).toBe("true");
    expect(button.attributes("aria-disabled")).toBe("true");
  });

  it("passes its own attributes (aria-labelledby) to the switch", () => {
    const wrapper = mount(HToggle, {
      props: { modelValue: false },
      attrs: { "aria-labelledby": "lbl" },
    });
    expect(wrapper.get("button").attributes("aria-labelledby")).toBe("lbl");
  });
});
