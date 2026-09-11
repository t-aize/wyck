/**
 * Identifiants MCP (url + token), lus depuis `~/.aurum/settings.json`.
 *
 * Pas de validation de forme ici : une URL / un token invalide échoue au
 * premier appel réseau plutôt qu'à la lecture du fichier.
 */
export interface CtraderClientConfig {
  /** URL du serveur MCP (ex. `https://mcp.ctrader.com/trading/mcp`). */
  url: string;
  /** Bearer token cTrader. */
  token: string;
}
