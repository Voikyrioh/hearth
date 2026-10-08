import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HButton from "./HButton.vue";

// HRT-40 (C42) : quand la raison est écrite sous le bouton, elle n'est dite qu'une fois (revue de la PR #59) :
// pas d'infobulle redondante, le texte visible décrit le bouton.
describe("HButton reason-below", () => {
  const props = { disabled: true, hint: "Seul un administrateur peut le faire" };

  it("écrit la raison sous le bouton, sans infobulle, et la lie au bouton", () => {
    const wrapper = mount(HButton, {
      props: { ...props, reasonBelow: true },
      slots: { default: "Agir" },
    });
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
    const reason = wrapper.get("[data-button-reason]");
    expect(reason.attributes("id")).toBeTruthy();
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(reason.attributes("id"));
    expect(wrapper.text().split(props.hint).length - 1).toBe(1);
  });

  it("sans reason-below, la raison reste une infobulle décrivant le bouton", () => {
    const wrapper = mount(HButton, { props, slots: { default: "Agir" } });
    const bubble = wrapper.get('[role="tooltip"]');
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(bubble.attributes("id"));
    expect(wrapper.find("[data-button-reason]").exists()).toBe(false);
  });
});
