import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HTabs from "./HTabs.vue";

const TABS = [
  { id: "detail", label: "Détail" },
  { id: "logs", label: "Journaux", disabled: true, describedBy: "why" },
] as const;

function make(selected: "detail" | "logs" = "detail") {
  return mount(HTabs, {
    props: { modelValue: selected, tabs: TABS, label: "Sections", idBase: "x" },
    attachTo: document.body,
  });
}

describe("HTabs", () => {
  it("tablist, un seul arrêt de tabulation, aria-controls vers les panneaux", () => {
    const wrapper = make();
    expect(wrapper.get("[role=tablist]").attributes("aria-label")).toBe("Sections");
    const tabs = wrapper.findAll("[role=tab]");
    expect(tabs.map((tab) => tab.attributes("tabindex"))).toEqual(["0", "-1"]);
    expect(tabs[0]?.attributes("aria-selected")).toBe("true");
    expect(tabs[0]?.attributes("aria-controls")).toBe("x-panel-detail");
    wrapper.unmount();
  });

  it("un onglet désactivé reste atteignable aux flèches, dit pourquoi, et ne s'active jamais", async () => {
    const wrapper = make();
    const tabs = wrapper.findAll("[role=tab]");
    (tabs[0]?.element as HTMLElement | undefined)?.focus();
    await wrapper.get("[role=tablist]").trigger("keydown", { key: "ArrowRight" });
    expect(document.activeElement).toBe(tabs[1]?.element);
    expect(tabs[1]?.attributes("aria-disabled")).toBe("true");
    expect(tabs[1]?.attributes("aria-describedby")).toBe("why");
    await tabs[1]?.trigger("click");
    await tabs[1]?.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    wrapper.unmount();
  });

  it("un onglet actif émet son identifiant au clic", async () => {
    const wrapper = mount(HTabs, {
      props: {
        modelValue: "b",
        tabs: [
          { id: "a", label: "A" },
          { id: "b", label: "B" },
        ],
        label: "L",
        idBase: "y",
      },
    });
    await wrapper.findAll("[role=tab]")[0]?.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toEqual([["a"]]);
    wrapper.unmount();
  });
});
