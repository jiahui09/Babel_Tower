import "i18next";

import { defaultNamespace, resources } from "./index";

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: typeof defaultNamespace;
    resources: (typeof resources)["zh-CN"];
  }
}

