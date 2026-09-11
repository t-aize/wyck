# `src/news`

Calendrier économique ForexFactory, profil d'un symbole, filtrage des
événements et biais macro (haussier / baissier / neutre) selon la classe d'actif.

## Source du calendrier

Le flux vient de `https://nfs.faireconomy.media/ff_calendar_thisweek.json` : un
miroir communautaire du calendrier ForexFactory, **pas une API officielle**. Pas
de SLA — le schéma, le rate-limit ou l'URL peuvent changer. Le fetch a un
timeout (10 s), un retry borné sur 429, un parse **event par event** (un objet
pourri n'élimine pas la semaine), et un cache disque à TTL (30 min) qui sert
aussi de repli si le réseau lâche.

## Architecture

```
src/news/
  calendar/                récupération + cache du flux ForexFactory
    schemas.ts             formes JSON (event, cache disque), parse permissif
    time.ts                fuseau Paris (affichage)
    cache.ts               lecture / écriture du cache TTL
    fetch.ts               HTTP, timeout, retry 429, `fetchCalendar`
  profile/                 « de quel instrument parle-t-on ? »
    types.ts               `AssetClass`, `NewsProfile`
    aliases.ts             tables (indices, métaux, crypto, devises FF / ISO)
    symbol.ts              normalisation du ticker, base/quote
    classify.ts            forex / métal / indice / crypto / énergie
    profile.ts             `newsProfile()` — pays + mots-clés
  analysis/                « que faire de cet event pour cet instrument ? »
    figures.ts             parse des forecast/previous ("255K", "0.3%", "2.48T")
    polarity.ts            nature de l'indicateur (croissance, inflation, taux, chômage)
    relevance.ts           l'event concerne-t-il le symbole ?
    bias.ts                haussier / baissier / neutre
```

Les tests collent au dossier qu'ils couvrent (`profile/profile.test.ts`, etc.).

## Usage

```ts
import { instrumentBias } from "./news/analysis/bias.ts";
import { isDefaultVisible } from "./news/analysis/relevance.ts";
import { fetchCalendar } from "./news/calendar/fetch.ts";
import { newsProfile } from "./news/profile/profile.ts";

const profile = newsProfile({ symbolName: "US100", base: "US100", quote: "USD" });
const events = await runtime.runPromise(fetchCalendar({ cacheDir: APP_DATA_DIR }));
const visible = events.filter((e) => isDefaultVisible(e, profile));
const bias = instrumentBias(visible[0]!, profile); // "bullish" | "bearish" | "neutral" | undefined
```
