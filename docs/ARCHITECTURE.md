# Architecture & conventions

Ce document reconstitue les conventions du projet — jusqu'ici éparpillées dans des commentaires
citant "cf. AUDIT_EFFECT.md §X.Y", un document qui n'a en réalité jamais existé dans le repo (audit
mené fin 2026, confirmé absent de l'historique git). Les numéros de section ci-dessous reprennent
tels quels ceux déjà cités dans le code, pour que les commentaires existants restent valides une
fois pointés ici plutôt que vers un fichier fantôme.

Portée : `src/domain/**` et `src/ctrader/**` sont écrits en Effect ; `src/ui/**` (composants et
hooks React/OpenTUI) consomme ces Effects mais reste en `async`/`await` — cf. §2.1.

## 1. Erreurs

### §1.4 — Une `Data.TaggedError` par cas d'échec distinct

Chaque ancien `throw new Error(...)` devient une sous-classe dédiée (`domain/trading.ts`,
`ctrader/client.ts`, `domain/news.ts`), toujours sous-classe d'`Error` (pas juste `TaggedError` nu)
pour que `toMessage()` (`src/errors.ts`, `instanceof Error ? error.message : String(error)`)
continue de fonctionner sans changement côté appelants. Permet à un appelant de faire
`Effect.catchTag(...)` sur un cas précis plutôt que de parser un message — utilisé en pratique dans
`ctrader/client.ts#callWithRetry` et `domain/news.ts#fetchWithRetry` (retry ciblé sur un tag précis).
Les champs structurés (`computedVolumeLots`, `symbolId`, `retryAfterSeconds`…) sont là pour un futur
consommateur qui voudrait réagir précisément à un cas — aujourd'hui, tout le reste du code collapse
sur `.message` à la première frontière `catch`/`.then(_, onFailed)`, et c'est un choix assumé, pas
un oubli : pas de branchement spéculatif sans consommateur réel.

## 2. Structure des Effects

### §2.1 — `Effect.gen` réservé à `domain/` et `ctrader/client.ts`

`Effect.gen` (et la composition Effect en général) ne sert que dans la logique métier et à la
frontière de transport MCP. Chaque hook de `src/ui/hooks/*.ts` reste en `async function`/`await`/
`try{}catch{}`, et convertit en chaîne via `toMessage()` dès qu'un Effect échoue — il n'y a jamais de
`Effect.gen` côté React, seulement `Effect.runPromise(...)` (ou, depuis §7, `fsRuntime.runPromise(...)`)
pour dérouler un Effect déjà entièrement construit.

Deux fonctions pures de `domain/trading.ts` (`computeVolume`, `validateRiskPercent`) restent en
`Effect.gen`/`Effect.fail` malgré l'absence de `yield*` ou d'effet de bord : elles sont `yield*`ées
depuis `prepareTrade`/`prepareAtrTrade`, des pipelines Effect plus larges — les garder Effect-shaped
sert la composition (`yield*` direct), ce n'est pas de la cérémonie gratuite.

### §2.2 — `Effect.forEach` avec pairage `{item, ok}` pour une opération "N, tolérante à l'échec unitaire"

Pattern utilisé dans `useOrderActions.ts#confirmPendingCancel` et `useAtrOrderTracking.ts` : chaque
résultat porte directement l'item d'origine (`.pipe(Effect.as({item, ok:true}), Effect.catchAll(() =>
Effect.succeed({item, ok:false})))`) plutôt que d'associer deux tableaux par index — plus robuste si
jamais l'un des deux tableaux venait à diverger. Ne s'applique pas partout : la commande `refresh`
utilise `Promise.all([refreshMarket(), refreshNews(), refreshTrend()])` pour "attendre que ces 3
appels indépendants se terminent", ce qui est correct ici — chacun avale déjà sa propre erreur en
interne (chaque hook gère son propre `error` state), donc il n'y a pas de pairage résultat/erreur à
faire au niveau de l'appelant.

### §2.3 / §6.1 — Suivi d'une opération en vol pour éviter une fuite de ressource

`ctrader/client.ts` garde une référence à la promesse `connect()` en cours (`#connecting`) : sans ce
suivi, un `close()` qui arrive pendant qu'un `connect()` est encore en vol trouve `#connected` encore
à `false`, ne fait rien, puis `connect()` finit par résoudre en arrière-plan sur un transport que
plus personne ne referme jamais. `close()` attend donc explicitement `#connecting` avant de statuer.

### §2.4 — `.then` dans les handlers déclenchés depuis le rendu, `async`/`await` partout ailleurs

Convention propre à `useOrderActions.ts` (et désormais aux hooks issus de son éclatement, cf. §8) :
les callbacks qui répondent directement à une action utilisateur (soumission de commande, clic de
confirmation) utilisent `.then(onSuccess, onFailure)` plutôt que `async`/`await`, parce qu'ils
tournent dans un contexte déjà synchrone déclenché par React. Le reste des hooks (`useMarketData`,
`useTrend`, `useCalendar`, `useCtraderConnection`) utilisent `async`/`await` — ce sont des effects de
polling/montage, pas des réactions directes à un événement.

### §2.5 / §3.3 — `useInterval` : le remplacement Effect-Fiber de `setInterval`

`src/ui/hooks/useInterval.ts` — `Effect.repeat(tick, Schedule.spaced(...))` lancé via
`Effect.runFork`, nettoyé via `Fiber.interrupt` au démontage (vérifié explicitement qu'aucun tick
fantôme ne survient après cleanup). C'est le mécanisme de polling attendu pour toute donnée qui doit
se rafraîchir périodiquement (`useMarketData`, `useTrend`, `useCalendar`).

Exception assumée : `useClock.ts` (tick d'1s) et `useTerminalShortcuts.ts` (timers one-shot
armé/désarmé pour Ctrl+C et le feedback temporaire) restent sur `setInterval`/`setTimeout` bruts —
ce n'est pas du "polling avec cleanup coûteux à rater", juste un tick d'horloge et deux timers
ponctuels ; le fiber Effect n'y apporterait rien.

## 3. Retries

### §3.1 — Retry sur rate-limit, respect du `retry-after` serveur

`domain/news.ts#fetchWithRetry` — retry ciblé via `Effect.catchTag("CalendarRateLimited", ...)`,
jusqu'à `MAX_RATE_LIMIT_RETRIES` fois, en attendant le délai `retryAfterSeconds` renvoyé par le
serveur (`Effect.sleep`). Toute autre erreur (HTTP non-200, JSON invalide, réseau down) n'est pas
retentée — pas de valeur à rejouer une 404 ou un payload cassé immédiatement.

### §3.2 — Retry exponentiel, lecture seule, jamais sur l'écriture

`ctrader/client.ts#callWithRetry` — `Schedule.exponential("200 millis") + Schedule.recurs(2)`,
appliqué uniquement aux méthodes de lecture (`getBalance`, `getSpotPrices`, `getTrendbars`,
`getPositions`…), et seulement sur `CtraderCallFailed` (échec de transport, pas un échec déjà rendu
par le serveur qu'un replay ne changerait pas). **Jamais** sur `createOrder`/`amendOrder`/
`cancelOrder`/`amendPosition`/`closePosition` : rejouer un ordre après un simple timeout réseau
risquerait de le dupliquer côté serveur si la première tentative avait en fait réussi.

## 4. Injection de dépendances

### §4.1 — `Context.Tag`/`Layer` réservés à `prepareTrade`/`prepareAtrTrade`

`CtraderClient` (le tag, `ctrader/client.ts`) n'est résolu par injection Effect
(`yield* CtraderClient`) que dans `domain/trading.ts#prepareTrade`/`prepareAtrTrade`, via la `Layer`
fournie au `ManagedRuntime` construit dans `App.tsx` (`Layer.succeed(CtraderClient, client)`). Tous
les autres consommateurs (les hooks) reçoivent l'instance `CtraderClientLive` directement en
paramètre — pas de DI là où un paramètre simple suffit. Résultat assumé : un hook qui a besoin des
deux (ex. `useOrderActions.ts`, avant son éclatement en §8) accepte `client` ET `runtime` comme deux
opts séparées, plutôt que de forcer tout le monde à passer par le tag pour un seul appelant qui en a
besoin.

### §4.2 — Le module `Config` d'Effect ne convient pas à un fichier JSON chiffré sur disque

`config.ts` — malgré le nom qui pourrait suggérer l'usage naturel, `Effect.Config` cible des
variables d'environnement, pas un fichier local. `config.ts` utilise `FileSystem.FileSystem`
directement à la place (cf. §7.1).

## 5. Validation

### §5.1 — `Schema` d'Effect pour une coercion scalaire, zod pour les schémas d'objet aux frontières

`domain/commands.ts` utilise `Schema.NumberFromString` (Effect) pour un seul usage : coercer une
chaîne d'argument CLI en nombre validé — pas un vrai schéma, juste une coercion ponctuelle réutilisée
5 fois plutôt que 5 `Number(raw); if(!Number.isFinite(raw))` dupliqués. Le tokenizing/parsing de
flags reste du code impératif ordinaire (pas un bon fit pour `Schema`). Partout ailleurs où un objet
externe non fiable doit être validé — `config.json` (`config.ts`), les réponses du serveur MCP
(`ctrader/schemas.ts`), le calendrier ForexFactory (`domain/news.ts`) — c'est zod qui est utilisé.
Deux bibliothèques, mais pour deux jobs différents : coercion scalaire ponctuelle vs schéma d'objet à
une frontière externe réelle. Ce n'est pas une confusion à résoudre, juste ne pas se mettre à utiliser
`Schema` pour un vrai schéma d'objet ni zod pour une coercion d'un seul champ.

### §5.3 — Un seul schéma, réutilisé à chaque point d'entrée qui en a besoin

`config.ts#AppConfigSchema` valide à la fois la saisie utilisateur dans `SetupScreen.tsx` et la
relecture de `config.json` — les deux points d'entrée s'accordent par construction (même schéma),
pas par coïncidence (deux validations écrites à la main qui pourraient diverger).

## 6. Sécurité des ressources

Cf. §2.3 ci-dessus (suivi de `connect()` en vol dans `ctrader/client.ts`) — même principe : ne jamais
laisser une opération asynchrone en vol sans qu'un cleanup concurrent puisse la retrouver.

## 7. Runtime & composition

### §7 — Un point de composition de `Layer` par service transversal, pas un `Effect.provide` par appel

Deux `ManagedRuntime` vivent dans l'app, chacun composé une seule fois :
- `runtime` (dans `App.tsx`, `ManagedRuntime.make(Layer.succeed(CtraderClient, client))`) — pour
  `prepareTrade`/`prepareAtrTrade` (cf. §4.1).
- `fsRuntime` (`src/effectRuntime.ts`, `ManagedRuntime.make(BunFileSystem.layer)`) — pour tout ce qui
  touche le système de fichiers (`config.ts`, `domain/news.ts`).

Avant l'introduction de `fsRuntime`, `Effect.provide(effect, BunFileSystem.layer)` était répété à
chaque site d'appel (`App.tsx`, `SetupScreen.tsx`) — la doc Effect elle-même recommande un seul point
de `provide` par appli plutôt que dispersé.

### §7.1 — Tout le système de fichiers passe par le service `FileSystem`, jamais `Bun.file`/`Bun.write`/`node:fs` en direct

`config.ts` l'a toujours fait ; `domain/news.ts` (cache du calendrier économique) contournait encore
`Bun.file()`/`Bun.write()` directement jusqu'à ce que ce soit aligné — c'était le seul point du code à
le faire, et le seul chemin d'I/O fichier qui n'avait pas de test en conséquence (un fake `Layer`
suffit à tester lecture/écriture sans toucher le vrai disque, cf. `config.test.ts`/`news.test.ts`).

## 8. Organisation des fichiers

### `domain/smc/` : un fichier par méthode/filtre, pas un fichier qui grossit indéfiniment

`trend.ts` regroupait à l'origine 7 algorithmes distincts (fractale/swing, ATR, méthode structurelle,
méthode événementielle BOS/CHoCH, liquidity sweep, EMA stack, ADX) dans un seul fichier de 500+
lignes. Éclaté en fichiers à responsabilité unique : `types.ts` (le type `Trend`), `series.ts` (true
range/moyenne mobile bas niveau), `swings.ts` (détection de fractale, partagée par les 3 méthodes qui
en dérivent), `atr.ts`, `structuralTrend.ts` (méthode 1), `structureEvents.ts` (méthode 2),
`sweep.ts`, `filters.ts` (EMA+ADX, explicitement pas du SMC). `trend.ts` ne garde que l'agrégateur
(`computeTrendState`) et ré-exporte tout le reste — **aucun import en dehors de `domain/smc/` n'a
besoin de changer** : `trend.ts` reste le point d'entrée public. Toute nouvelle notion SMC (order
block, FVG, équilibre/premium-discount…) devrait suivre le même principe : un nouveau fichier, pas un
ajout à un fichier existant qui ne lui est pas dédié.

### `useOrderActions.ts` : composition de hooks à responsabilité unique

Éclaté en `useTradeConfirm.ts`, `useModifyConfirm.ts`, `useCancelConfirm.ts` (un état de confirmation
pendante chacun) et `useCommandRouter.ts` (parsing + dispatch des commandes, appelle les 3 précédents
sur succès) — `useOrderActions.ts` devient une simple composition qui retourne la même forme
qu'avant. Un nouveau flux de confirmation (ex. une feature SMC) ajoute un `useXConfirm.ts` + un cas
dans `useCommandRouter.ts`, sans toucher `ConnectedApp` ni faire regrossir un fichier unique.

### État transversal : Context React, pas des paramètres enfilés à travers plusieurs hooks

`FeedbackContext` (`feedback`/`setFeedback`) et `CtraderContext` (`client`/`runtime`/`connected`/
`symbolId`/`connectionError`) — les deux seules données réellement transversales (consommées par
plusieurs hooks indépendants qui ne peuvent pas se les passer directement) vivent dans
`src/ui/context/`. Tout le reste (`positions`, `trendRows`, `atrRaw`…) reste de la composition
normale (1-2 sauts dans le corps de `ConnectedApp`) — ne pas ajouter de Context pour un problème qui
n'existe pas encore à cette échelle.
