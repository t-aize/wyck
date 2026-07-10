import "dotenv/config";
import { createEnv } from "@t3-oss/env-core";
import { z } from "zod";
import { TRENDBAR_PERIODS } from "./constants.ts";

export const env = createEnv({
  server: {
    CTRADER_MCP_URL: z.url({ message: "CTRADER_MCP_URL doit être une URL valide" }),
    CTRADER_MCP_TOKEN: z.string().min(1, "CTRADER_MCP_TOKEN est requis"),

    SYMBOL: z.string().min(1).default("XAUUSD"),
    ATR_PERIOD: z.coerce.number().int().positive().default(14),
    ATR_TIMEFRAME: z.enum(TRENDBAR_PERIODS).default("H_1"),
    ATR_MULTIPLIER: z.coerce.number().positive().default(1.5),
    DEFAULT_RR: z.coerce.number().positive().default(1.2),
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
