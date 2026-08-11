/** Type primitif partagé par tout `domain/smc/**` — fichier séparé (pas dans trend.ts) pour éviter
 * un cycle d'import : plusieurs modules (structuralTrend.ts, structureEvents.ts) en ont besoin, et
 * trend.ts (l'agrégateur) importe lui-même depuis ces modules. */
export type Trend = -1 | 0 | 1;
