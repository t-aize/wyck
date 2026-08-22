/**
 * — Sessions (`MARKET_SESSIONS`) : horaires d'ouverture usuels des quatre places qui, mises
 *   bout à bout, couvrent le marché 24h/24 (Sydney, Tokyo, Londres, New York — cf. sources).
 * — Killzones ICT (`KILLZONES`) : sous-fenêtres plus étroites, à forte probabilité de mouvement
 *   selon la méthodologie ICT, toutes définies en heure de New York par convention. Au plus une
 *   active à la fois (fenêtres disjointes).
 *
 * Sources (horaires locaux usuels, confirmés par recherche) :
 * - Sydney   08:00-17:00 AEST/AEDT (Australia/Sydney)
 * - Tokyo    09:00-18:00 JST, pas de DST (Asia/Tokyo)
 * - Londres  08:00-17:00 GMT/BST (Europe/London)
 * - New York 08:00-17:00 EST/EDT (America/New_York)
 */

import type { Killzone, MarketSession } from "./types.ts";
import { PLACE_LABEL } from "./types.ts";

export const MARKET_SESSIONS: MarketSession[] = [
  {
    id: "sydney",
    label: PLACE_LABEL.sydney,
    timeZone: "Australia/Sydney",
    startHour: 8,
    endHour: 17,
  },
  { id: "tokyo", label: PLACE_LABEL.tokyo, timeZone: "Asia/Tokyo", startHour: 9, endHour: 18 },
  { id: "london", label: PLACE_LABEL.london, timeZone: "Europe/London", startHour: 8, endHour: 17 },
  {
    id: "newYork",
    label: PLACE_LABEL.newYork,
    timeZone: "America/New_York",
    startHour: 8,
    endHour: 17,
  },
];

export const KILLZONES: Killzone[] = [
  // Pas de place unique correspondante (chevauche Sydney/Tokyo) : label indépendant.
  { id: "asia", label: "ASIA", startHour: 20, endHour: 24 },
  { id: "london", label: PLACE_LABEL.london, startHour: 2, endHour: 5 },
  { id: "newYork", label: PLACE_LABEL.newYork, startHour: 7, endHour: 10 },
  { id: "londonClose", label: `${PLACE_LABEL.london} CLOSE`, startHour: 10, endHour: 12 },
];
