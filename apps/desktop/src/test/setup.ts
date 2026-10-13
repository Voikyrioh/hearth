import { enableAutoUnmount } from "@vue/test-utils";
import { afterEach } from "vitest";

// Les bulles sont téléportées dans `body` : sans démontage, celles d'un test resteraient dans le suivant.
enableAutoUnmount(afterEach);
