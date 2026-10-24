import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HInput from "./HInput.vue";
import HMeter from "./HMeter.vue";
import HTextarea from "./HTextarea.vue";

// Ce que le formulaire de déploiement a demandé aux atomes (HRT-69).
describe("HInput : libellé masqué, mode de saisie, sortie du champ", () => {
  it("garde le libellé pour les lecteurs d'écran seulement et le lie au champ", () => {
    const wrapper = mount(HInput, {
      props: {
        modelValue: "",
        label: "Port du serveur",
        hideLabel: true,
        inputmode: "numeric",
        maxlength: 5,
      },
    });
    const label = wrapper.get("label");
    expect(label.classes()).toContain("sr-only");
    expect(label.attributes("for")).toBe(wrapper.get("input").attributes("id"));
    expect(wrapper.get("input").attributes("inputmode")).toBe("numeric");
    expect(wrapper.get("input").attributes("maxlength")).toBe("5");
  });

  it("garde un libellé visible par défaut et annonce la sortie du champ", async () => {
    const wrapper = mount(HInput, { props: { modelValue: "", label: "Nom" } });
    expect(wrapper.get("label").classes()).not.toContain("sr-only");
    await wrapper.get("input").trigger("blur");
    expect(wrapper.emitted("blur")).toHaveLength(1);
  });
});

describe("HTextarea", () => {
  it("a un libellé visible, une erreur annoncée et grandit avec la saisie jusqu'à son maximum", async () => {
    const wrapper = mount(HTextarea, {
      props: {
        modelValue: "",
        label: "Commande de lancement",
        error: "Guillemets non fermés",
        rows: 3,
        maxRows: 5,
      },
    });
    expect(wrapper.get("label").text()).toBe("Commande de lancement");
    expect(wrapper.get("textarea").attributes("aria-invalid")).toBe("true");
    expect(wrapper.get('[role="alert"]').text()).toBe("Guillemets non fermés");
    expect(wrapper.get("textarea").attributes("rows")).toBe("3");
    await wrapper.setProps({ modelValue: "a\nb\nc\nd" });
    expect(wrapper.get("textarea").attributes("rows")).toBe("4");
    await wrapper.setProps({ modelValue: "1\n2\n3\n4\n5\n6\n7" });
    expect(wrapper.get("textarea").attributes("rows")).toBe("5");
  });

  it("émet la saisie et la sortie du champ", async () => {
    const wrapper = mount(HTextarea, { props: { modelValue: "", label: "Commande" } });
    await wrapper.get("textarea").setValue("run");
    await wrapper.get("textarea").trigger("blur");
    expect(wrapper.emitted("update:modelValue")).toEqual([["run"]]);
    expect(wrapper.emitted("blur")).toHaveLength(1);
  });
});

describe("HMeter : barre indéterminée", () => {
  it("glisse une barre quand la progression n'est pas connue, une jauge sinon", () => {
    const unknown = mount(HMeter, {
      props: { ratio: null, indeterminate: true, label: "Récupération" },
    });
    expect(unknown.find(".meter__slide").exists()).toBe(true);
    expect(unknown.find(".meter__value").exists()).toBe(false);
    const known = mount(HMeter, {
      props: { ratio: 0.4, indeterminate: true, label: "Récupération" },
    });
    expect(known.find(".meter__slide").exists()).toBe(false);
    expect(known.find(".meter__value").exists()).toBe(true);
    const empty = mount(HMeter, { props: { ratio: null, label: "Disque" } });
    expect(empty.find(".meter__slide").exists()).toBe(false);
  });
});
