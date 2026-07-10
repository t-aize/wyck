import "dotenv/config";
import { createEnv } from "@t3-oss/env-core";
import { z } from "zod";

export const env = createEnv({
  server: {
    CTRADER_MCP_URL: z.url({ message: "CTRADER_MCP_URL doit être une URL valide" }),
    CTRADER_MCP_TOKEN: z.string().min(1, "CTRADER_MCP_TOKEN est requis"),
  },
  runtimeEnv: process.env,
  emptyStringAsUndefined: true,
  onValidationError: (issues) => {
    console.error("✖ Configuration invalide (.env) :");
    for (const issue of issues) {
      const path = issue.path
        ?.map((segment) => (typeof segment === "object" ? segment.key : segment))
        .join(".");
      console.error(`  - ${path ?? "(racine)"}: ${issue.message}`);
    }
    console.error("  Vérifie ton fichier .env (voir README.md → Configuration, ou .env.example).");
    process.exit(1);
  },
});
