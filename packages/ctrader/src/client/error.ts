/**
 * Échec d'un appel à un outil MCP cTrader.
 *
 * Une seule classe — transport (réseau / DNS / timeout), erreur explicite de
 * l'outil, réponse vide, contenu non-JSON, ou schéma zod inattendu. L'ancienne
 * hiérarchie de sous-types tagués n'était discriminée par aucun appelant (tous
 * affichent `.message`). Seule la distinction « échec de transport, donc
 * retryable » servait réellement, portée ici par {@link CtraderMcpError.retryable}.
 */

/**
 * Erreur d'un appel MCP.
 *
 * @param message - Texte affichable (l'app passe par `toMessage`).
 * @param retryable - `true` seulement pour un échec de transport (réseau, 5xx,
 *   timeout). Un 4xx (token, requête malformée) ne se répare pas en rejouant.
 */
export class CtraderMcpError extends Error {
  constructor(
    message: string,
    readonly retryable = false,
  ) {
    super(message);
    this.name = "CtraderMcpError";
  }
}
