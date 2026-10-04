import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import EmptyState from "./EmptyState.vue";

describe("EmptyState", () => {
  it("shows title and text, without optional slots", () => {
    const wrapper = mount(EmptyState, { props: { title: "Titre", text: "Texte" } });
    expect(wrapper.get("h1").text()).toBe("Titre");
    expect(wrapper.get("p").text()).toBe("Texte");
    expect(wrapper.find(".empty__illustration").exists()).toBe(false);
    expect(wrapper.find(".empty__action").exists()).toBe(false);
  });

  it("renders the illustration and action slots", () => {
    const wrapper = mount(EmptyState, {
      props: { title: "Titre", text: "Texte" },
      slots: { illustration: "<i>img</i>", action: "<button>Go</button>" },
    });
    expect(wrapper.find(".empty__illustration i").exists()).toBe(true);
    expect(wrapper.get(".empty__action button").text()).toBe("Go");
  });
});
