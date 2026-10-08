import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { h } from "vue";
import HButton from "./HButton.vue";
import HCheckbox from "./HCheckbox.vue";
import HIcon from "./HIcon.vue";
import HInput from "./HInput.vue";
import HPasswordInput from "./HPasswordInput.vue";
import HSegmented from "./HSegmented.vue";
import HSpinner from "./HSpinner.vue";
import HTag from "./HTag.vue";
import HTooltip from "./HTooltip.vue";

describe("HButton : la raison du blocage écrite sous le bouton (HRT-40, C42)", () => {
  it("écrit la raison en texte permanent quand `reason-below` et désactivé, rien sinon", () => {
    const blocked = mount(HButton, {
      props: {
        disabled: true,
        hint: "Seul un administrateur peut mettre à jour l'agent",
        reasonBelow: true,
      },
      slots: { default: "Mettre à jour l'agent" },
    });
    expect(blocked.get("[data-button-reason]").text()).toBe(
      "Seul un administrateur peut mettre à jour l'agent",
    );
    const open = mount(HButton, {
      props: { hint: "Seul un administrateur peut mettre à jour l'agent", reasonBelow: true },
    });
    expect(open.find("[data-button-reason]").exists()).toBe(false);
    const plain = mount(HButton, { props: { disabled: true, hint: "x" } });
    expect(plain.find("[data-button-reason]").exists()).toBe(false);
  });
});

describe("HButton variants and busy state", () => {
  it("renders the danger variant, and a solid one only for danger", () => {
    const danger = mount(HButton, { props: { variant: "danger" } }).get("button");
    expect(danger.classes()).toContain("btn--danger");
    expect(danger.classes()).not.toContain("btn--solid");
    const solid = mount(HButton, { props: { variant: "danger", solid: true } }).get("button");
    expect(solid.classes()).toContain("btn--solid");
    const primarySolid = mount(HButton, { props: { solid: true } }).get("button");
    expect(primarySolid.classes()).not.toContain("btn--solid");
  });

  it("has three sizes", () => {
    for (const size of ["sm", "md", "lg"] as const) {
      expect(mount(HButton, { props: { size } }).get("button").classes()).toContain(`btn--${size}`);
    }
  });

  it("shows a spinner and ignores clicks while busy, and says so to assistive tech", async () => {
    const wrapper = mount(HButton, { props: { busy: true }, slots: { default: "Envoi" } });
    expect(wrapper.find("svg.spinner").exists()).toBe(true);
    expect(wrapper.get("button").attributes("aria-busy")).toBe("true");
    expect(wrapper.get("button").attributes("aria-disabled")).toBe("true");
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("click")).toBeUndefined();
  });
});

describe("HIcon", () => {
  const names = [
    "plus",
    "settings",
    "alert",
    "info",
    "check",
    "close",
    "refresh",
    "server",
    "eye",
    "eye-off",
  ] as const;

  it("draws every icon of the set as an SVG with at least one path", () => {
    for (const name of names) {
      const wrapper = mount(HIcon, { props: { name } });
      expect(wrapper.element.tagName.toLowerCase(), name).toBe("svg");
      expect(wrapper.findAll("path").length, name).toBeGreaterThan(0);
    }
  });

  it("is decorative without a label and an image with one", () => {
    expect(mount(HIcon, { props: { name: "plus" } }).attributes("aria-hidden")).toBe("true");
    const labelled = mount(HIcon, { props: { name: "plus", label: "Ajouter" } });
    expect(labelled.attributes("role")).toBe("img");
    expect(labelled.attributes("aria-label")).toBe("Ajouter");
  });
});

describe("HSpinner", () => {
  it("is decorative by default and a status when labelled", () => {
    expect(mount(HSpinner).attributes("aria-hidden")).toBe("true");
    expect(mount(HSpinner, { props: { label: "Chargement" } }).attributes("role")).toBe("status");
  });
});

describe("HInput", () => {
  it("links the label to the field and emits the typed value", async () => {
    const wrapper = mount(HInput, { props: { modelValue: "", label: "Adresse" } });
    const input = wrapper.get("input");
    expect(wrapper.get("label").attributes("for")).toBe(input.attributes("id"));
    await input.setValue("192.168.1.1");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["192.168.1.1"]);
  });

  it("shows help, and an error that marks the field invalid and is described", () => {
    const wrapper = mount(HInput, {
      props: { modelValue: "", label: "Adresse", help: "IP ou nom", error: "Adresse invalide" },
    });
    const input = wrapper.get("input");
    expect(input.attributes("aria-invalid")).toBe("true");
    const describedBy = input.attributes("aria-describedby") ?? "";
    expect(describedBy.split(" ")).toHaveLength(2);
    expect(wrapper.get('[role="alert"]').text()).toBe("Adresse invalide");
    expect(wrapper.text()).toContain("IP ou nom");
  });

  it("has no error attributes when valid", () => {
    const input = mount(HInput, { props: { modelValue: "", label: "Nom" } }).get("input");
    expect(input.attributes("aria-invalid")).toBeUndefined();
    expect(input.attributes("aria-describedby")).toBeUndefined();
  });
});

describe("HPasswordInput", () => {
  it("hides the password, and toggles it with an explained button", async () => {
    const wrapper = mount(HPasswordInput, {
      props: { modelValue: "secret", label: "Mot de passe" },
    });
    expect(wrapper.get("input").attributes("type")).toBe("password");
    const toggle = wrapper.get("button");
    expect(toggle.attributes("aria-label")).toBe("Afficher le mot de passe");
    await toggle.trigger("click");
    expect(wrapper.get("input").attributes("type")).toBe("text");
    expect(toggle.attributes("aria-label")).toBe("Masquer le mot de passe");
    expect(toggle.attributes("aria-pressed")).toBe("true");
  });
});

describe("HCheckbox", () => {
  it("is a real checkbox with its label, and emits the new value", async () => {
    const wrapper = mount(HCheckbox, { props: { modelValue: false, label: "Se souvenir de moi" } });
    expect(wrapper.text()).toBe("Se souvenir de moi");
    await wrapper.get("input").setValue(true);
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([true]);
  });

  it("can be disabled", () => {
    const wrapper = mount(HCheckbox, { props: { modelValue: true, label: "x", disabled: true } });
    expect(wrapper.get("input").attributes("disabled")).toBeDefined();
  });
});

describe("HTag", () => {
  it("renders its text with a tone class", () => {
    const wrapper = mount(HTag, {
      props: { tone: "accent" },
      slots: { default: "Administrateur" },
    });
    expect(wrapper.text()).toBe("Administrateur");
    expect(wrapper.classes()).toContain("tag--accent");
  });
});

describe("HTooltip", () => {
  it("shows its bubble on hover and on keyboard focus, and closes on Escape", async () => {
    const wrapper = mount(HTooltip, {
      props: { text: "Explication" },
      slots: { default: '<button type="button">x</button>' },
    });
    const bubble = wrapper.get('[role="tooltip"]');
    expect((bubble.element as HTMLElement).style.display).toBe("none");
    await wrapper.trigger("mouseenter");
    expect((bubble.element as HTMLElement).style.display).not.toBe("none");
    await wrapper.trigger("mouseleave");
    await wrapper.get("button").trigger("focusin");
    expect((bubble.element as HTMLElement).style.display).not.toBe("none");
    await wrapper.trigger("keydown", { key: "Escape" });
    expect((bubble.element as HTMLElement).style.display).toBe("none");
  });

  it("hands the bubble id to the control so it is read aloud", () => {
    const wrapper = mount(HTooltip, {
      props: { text: "Explication" },
      slots: {
        default: ({ describedby }: { describedby?: string }) =>
          h("button", { type: "button", "aria-describedby": describedby }, "x"),
      },
    });
    const id = wrapper.get('[role="tooltip"]').attributes("id");
    expect(id).toBeTruthy();
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(id);
  });
});

describe("HSegmented", () => {
  const options = [
    { value: "a", label: "Un" },
    { value: "b", label: "Deux" },
    { value: "c", label: "Trois" },
  ] as const;

  it("is a radio group with one checked, tabbable option", () => {
    const wrapper = mount(HSegmented, { props: { modelValue: "b", options, label: "Choix" } });
    expect(wrapper.get('[role="radiogroup"]').attributes("aria-label")).toBe("Choix");
    const radios = wrapper.findAll('[role="radio"]');
    expect(radios.map((r) => r.attributes("aria-checked"))).toEqual(["false", "true", "false"]);
    expect(radios.map((r) => r.attributes("tabindex"))).toEqual(["-1", "0", "-1"]);
  });

  it("selects on click and moves with the arrow keys, wrapping around", async () => {
    const wrapper = mount(HSegmented, { props: { modelValue: "c", options, label: "Choix" } });
    await wrapper.findAll('[role="radio"]')[0]?.trigger("click");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["a"]);
    await wrapper.get('[role="radiogroup"]').trigger("keydown", { key: "ArrowRight" });
    expect(wrapper.emitted("update:modelValue")?.[1]).toEqual(["a"]);
    await wrapper.get('[role="radiogroup"]').trigger("keydown", { key: "ArrowLeft" });
    expect(wrapper.emitted("update:modelValue")?.[2]).toEqual(["b"]);
  });
});
