# `@aurum/news`

Paquet **privé** : calendrier économique ForexFactory, profil d'un symbole, filtrage des
événements et biais macro (haussier / baissier / neutre) selon la classe d'actif.

Rien ici n'est public. `private: true` empêche un `npm publish` accidentel.
`license: "UNLICENSED"` (identifiant SPDX) signifie **aucune licence accordée à
autrui** — c'est la convention npm pour du code propriétaire, à côté du
`LICENSE` racine (« tous droits réservés »). Les deux se complètent : `private`
bloque la publication, `UNLICENSED` documente l'absence de droit de réutilisation.

## Architecture

```
src/
  index.ts                 façade publique (réexport uniquement)
  calendar/                récupération + cache du flux ForexFactory
    schemas.ts             formes JSON (event, cache disque)
    time.ts                fuseau Paris (affichage et clé de cache)
    cache.ts               lecture / écriture du cache journalier
    fetch.ts               HTTP, retry 429, `fetchCalendar`
  profile/                 « de quel instrument parle-t-on ? »
    types.ts               `AssetClass`, `NewsProfile`
    aliases.ts             tables (indices, métaux, crypto, devises FF)
    symbol.ts              normalisation du ticker, base/quote
    classify.ts            forex / métal / indice / crypto / énergie
    profile.ts             `newsProfile()` — pays + mots-clés
  analysis/                « que faire de cet event pour cet instrument ? »
    figures.ts             parse des forecast/previous ("255K", "0.3%")
    polarity.ts            nature de l'indicateur (croissance, inflation, chômage)
    relevance.ts           l'event concerne-t-il le symbole ?
    bias.ts                haussier / baissier / neutre
```

Les tests collent au dossier qu'ils couvrent (`profile/profile.test.ts`, etc.).

## Usage (côté app)

```ts
import { fetchCalendar, newsProfile, isDefaultVisible, instrumentBias } from "@aurum/news";

const profile = newsProfile({ symbolName: "US100", base: "US100", quote: "USD" });
const events = await runtime.runPromise(fetchCalendar({ cacheDir: APP_DATA_DIR }));
const visible = events.filter((e) => isDefaultVisible(e, profile));
const bias = instrumentBias(visible[0]!, profile); // "bullish" | "bearish" | "neutral" | undefined
```
