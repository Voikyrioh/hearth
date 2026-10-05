import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import SettingRow from "./SettingRow.vue";

describe("SettingRow", () => {
  it("shows label, help and the control slot", () => {
    const wrapper = mount(SettingRow, {
      props: { label: "Libellé", help: "Aide" },
      slots: { default: "<button>Ok</button>" },
    });
    expect(wrapper.text()).toContain("Libellé");
    expect(wrapper.text()).toContain("Aide");
    expect(wrapper.get("button").text()).toBe("Ok");
  });

  it("omits the help when none is given", () => {
    const wrapper = mount(SettingRow, { props: { label: "Libellé" } });
    expect(wrapper.find(".row__help").exists()).toBe(false);
  });

  it("gives the control the id of the label to name itself with", () => {
    const wrapper = mount(SettingRow, {
      props: { label: "Libellé" },
      slots: {
        default: `<template #default="{ labelId }"><i :aria-labelledby="labelId" /></template>`,
      },
    });
    const labelId = wrapper.get("i").attributes("aria-labelledby");
    expect(labelId).toBeTruthy();
    expect(wrapper.get(`[id="${labelId}"]`).text()).toBe("Libellé");
  });
});
