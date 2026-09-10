/**
 * Échec d'un appel à un outil MCP cTrader.
 *
 * Une seule classe — transport (réseau / DNS / timeout), erreur explicite de
 * l'outil, réponse vide, ou contenu non-JSON. L'enveloppe MCP est déjà validée
 * par le SDK (`CallToolResultSchema`). Le JSON *métier* n'est plus re-parsé
 * par Zod : on lui fait confiance, ou on le mappe ({@link mapPosition}).
 *
 * Seule la distinction « retryable » compte pour le client : un 4xx (token,
 * requête malformée) ne se répare pas en rejouant.
 */

/**
 * @param message - Texte affichable (l'app passe par `toMessage`).
 * @param retryable - `true` seulement pour un échec de transport (réseau, 5xx,
 *   timeout). Un 4xx ne se répare pas en rejouant.
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
