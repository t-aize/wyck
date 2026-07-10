#!/usr/bin/env bun
/**
 * Liste les outils exposés par le serveur MCP cTrader configuré dans .env.
 * Usage : bun run mcp:tools
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import type { Tool } from "@modelcontextprotocol/sdk/types.js";
import { env } from "../src/env.ts";

const CLIENT_INFO = { name: "aurum-list-tools", version: "0.1.0" };

function fail(message: string, hint?: string): never {
  console.error(`✖ ${message}`);
  if (hint) console.error(`  ${hint}`);
  process.exit(1);
}

function describeError(error: unknown): string {
  if (error instanceof Error) {
    if (/401/.test(error.message)) {
      return `${error.message}\n  → Token expiré ou invalide : régénère-le depuis cTrader Web → Settings → Remote MCP.`;
    }
    return error.message;
  }
  return String(error);
}

async function collectAllTools(client: Client): Promise<Tool[]> {
  const tools: Tool[] = [];
  let cursor: string | undefined;

  do {
    const page = await client.listTools(cursor ? { cursor } : undefined);
    tools.push(...page.tools);
    cursor = page.nextCursor;
  } while (cursor);

  return tools;
}

function formatParams(tool: Tool): string | undefined {
  const schema = tool.inputSchema;
  const properties = schema?.properties ? Object.keys(schema.properties) : [];
  if (properties.length === 0) return undefined;

  const required = new Set(schema?.required ?? []);
  return properties.map((name) => (required.has(name) ? `${name}*` : name)).join(", ");
}

function printTools(tools: Tool[]): void {
  const sorted = [...tools].sort((a, b) => a.name.localeCompare(b.name));

  for (const tool of sorted) {
    console.log(`• ${tool.name}`);
    if (tool.description) console.log(`  ${tool.description}`);

    const params = formatParams(tool);
    if (params) console.log(`  params: ${params}`);

    console.log();
  }

  console.log(`* = paramètre requis`);
  console.log(`${sorted.length} outil${sorted.length > 1 ? "s" : ""} au total.`);
}

const transport = new StreamableHTTPClientTransport(new URL(env.CTRADER_MCP_URL), {
  requestInit: {
    headers: { Authorization: `Bearer ${env.CTRADER_MCP_TOKEN}` },
  },
});

const client = new Client(CLIENT_INFO);

try {
  await client.connect(transport);
} catch (error) {
  fail(`Connexion au serveur MCP échouée (${env.CTRADER_MCP_URL})`, describeError(error));
}

try {
  const server = client.getServerVersion();
  if (server) console.log(`Connecté à ${server.name} v${server.version}\n`);

  const tools = await collectAllTools(client);

  if (tools.length === 0) {
    console.log("Aucun outil exposé par ce serveur.");
  } else {
    printTools(tools);
  }
} catch (error) {
  fail("Erreur inattendue", describeError(error));
} finally {
  await client.close();
}
