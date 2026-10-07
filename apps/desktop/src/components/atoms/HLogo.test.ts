import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HIcon from "./HIcon.vue";
import HLogo from "./HLogo.vue";

describe("HLogo", () => {
  it("is announced as Hearth unless decorative", () => {
    expect(mount(HLogo).attributes("aria-label")).toBe("Hearth");
    const decorative = mount(HLogo, { props: { decorative: true } });
    expect(decorative.attributes("aria-hidden")).toBe("true");
    expect(decorative.attributes("aria-label")).toBeUndefined();
  });

  it("draws the same round-tipped flame at every size", () => {
    const drawings = (["sm", "md", "lg"] as const).map((size) =>
      mount(HLogo, { props: { size } })
        .findAll("path")
        .map((path) => path.attributes("d")),
    );
    expect(drawings[0]).toEqual(drawings[1]);
    expect(drawings[1]).toEqual(drawings[2]);
    // Pointe ronde : un arc de rayon 14 ferme le sommet de la flamme, pas de découpe interne.
    expect(drawings[0]?.[1]).toContain("A14 14");
    expect(drawings[0]?.[1]?.match(/M/g)).toHaveLength(1);
  });
});

describe("HIcon", () => {
  it("is hidden from assistive tech without a label", () => {
    expect(mount(HIcon, { props: { name: "plus" } }).attributes("aria-hidden")).toBe("true");
  });

  it("is an image when labelled", () => {
    const icon = mount(HIcon, { props: { name: "settings", label: "Réglages" } });
    expect(icon.attributes("role")).toBe("img");
    expect(icon.attributes("aria-label")).toBe("Réglages");
  });
});
