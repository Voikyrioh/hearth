import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import Welcome from "./Welcome.vue";

describe("Welcome (BR-CLIENT-013)", () => {
  it("shows the exact first-launch texts", () => {
    const wrapper = mount(Welcome);
    expect(wrapper.get("h1").text()).toBe("Bienvenue dans Hearth");
    expect(wrapper.text()).toContain("Ajoute ton premier serveur pour commencer.");
    expect(wrapper.get("button").text()).toBe("Ajouter un serveur");
  });

  it("offers the button but keeps it inactive with a tooltip", () => {
    const wrapper = mount(Welcome);
    expect(wrapper.get("button").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe("Bientôt disponible");
  });
});
