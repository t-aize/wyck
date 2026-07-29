# SMC complet — plan d'implémentation

## Contexte

`domain/structure.ts` implémente aujourd'hui **un morceau** du Smart Money Concepts : swing
high/low, biais, BOS/CHoCH, sweep — adapté de *"ALV - SMC MTF Structure"* (Pine v6). L'utilisateur
trade en réalité avec le **SMC complet façon LuxAlgo** : deux scripts distincts, collés dans cette
conversation :

1. **`Smart Money Concepts [LuxAlgo]`** (Pine v5) — structure interne + swing, Order Blocks,
   Fair Value Gaps, Equal Highs/Lows, niveaux MTF (D/W/M), zones Premium/Discount.
2. **`Trendlines with Breaks [LuxAlgo]`** (Pine v5) — un algo **séparé** : droites de tendance
   dynamiques à travers les pivots + détection de cassure de la droite elle-même (pas d'un niveau
   horizontal).

## Licence

Les deux scripts sont sous **CC BY-NC-SA 4.0** (Attribution-NonCommercial-ShareAlike), © LuxAlgo.
aurum est un projet perso/privé, non-commercial — compatible. On applique la même convention que
pour le script ALV déjà porté : attribution en tête de fichier (`Adapté de "..." (Pine, LuxAlgo)`),
jamais une recopie du code Pine lui-même, un portage fidèle du *comportement* en TypeScript pur et
testé.

## Ce qui existe vs ce qui manque

| Concept | État actuel |
|---|---|
| Swing high/low, biais, BOS/CHoCH | ✅ `computeStructure` (structure.ts), un seul `length` par timeframe (5M=8, 15M=7, 1H=14, 4H=20, D1=28) |
| Sweep (liquidity grab) | ✅ `sweepLow`/`sweepHigh` dans `computeStructure` |
| Structure **interne** (distincte du swing) | ❌ absent |
| Order Blocks | ❌ absent |
| Fair Value Gaps | ❌ absent |
| Equal Highs/Lows | ❌ absent |
| Zones Premium/Discount/Equilibrium | ❌ absent |
| Niveaux MTF (prev D/W/M high-low) | ❌ absent |
| Trendlines & cassure de droite | ❌ absent — algo différent de tout ce qui existe |

## Concept par concept — ce que fait vraiment le Pine, traduit en clair

### 1. Deux conventions de pivot différentes cohabitent déjà chez LuxAlgo lui-même

**Important à noter avant tout** : les deux scripts LuxAlgo n'utilisent **pas** la même méthode de
détection de pivot que l'un l'autre — et aucun des deux n'utilise exactement celle déjà portée
depuis le script ALV dans aurum. Il faut trancher consciemment, pas mélanger par accident.

- **aurum aujourd'hui** (`isPivotHigh`/`isPivotLow`, adapté d'ALV) : fenêtre **symétrique**
  `[i-length, i+length]` — un pivot n'est confirmé qu'une fois `length` bougies passées des deux
  côtés. Classique, sans repaint.
- **`Trendlines with Breaks`** : `ta.pivothigh(length,length)` / `ta.pivotlow(length,length)` —
  **exactement la même convention symétrique**. On peut réutiliser `isPivotHigh`/`isPivotLow` tels
  quels pour cette partie.
- **`Smart Money Concepts`** (structure interne/swing) : méthode **"leg"**, différente et
  **asymétrique/streaming** — à chaque bougie, compare `high[size]` (le high d'il y a `size`
  bougies) à `ta.highest(size)` (le plus haut sur la fenêtre courante de `size` bougies). Réagit
  plus vite qu'une fenêtre symétrique mais peut "flipper" avant de se stabiliser. C'est cette
  méthode qui alimente structure interne, structure swing, Order Blocks et Equal Highs/Lows dans
  le script SMC.

**Décision à prendre** : est-ce qu'on porte la méthode "leg" fidèlement (fidèle à LuxAlgo, mais un
deuxième algo de pivot à maintenir en plus de l'existant), ou est-ce qu'on réutilise
`isPivotHigh`/`isPivotLow` (déjà existant, déjà testé) partout, y compris pour ce que LuxAlgo
appelle structure interne/swing ? Les deux sont défendables ; la fidélité au script réel penche
pour "leg", la simplicité/dette penche pour réutiliser l'existant. *(à trancher ensemble)*

### 2. Structure interne vs structure swing (distinction clé, à ne pas confondre avec le MTF d'aurum)

Le script SMC calcule la structure **deux fois sur le même timeframe** à deux longueurs
différentes :
- **Interne** : longueur fixe **5** — capture les micro-changements de structure.
- **Swing** : longueur par défaut **50** — structure majeure.

C'est différent de ce qu'aurum fait déjà (une structure par **timeframe**, 5M/15M/1H/4H/D1). Les
deux logiques sont complémentaires, pas redondantes : LuxAlgo calcule interne+swing **sur un seul
chart** ; aurum calcule swing **sur 5 charts différents**. Rien n'empêche de faire les deux :
garder le multi-timeframe existant, et ajouter une structure "interne" (longueur courte, ex. 5) en
plus de la structure "swing" (longueur actuelle du TF) sur chacun des 5 timeframes déjà en place.

**Décision à prendre** : ajoute-t-on l'interne sur les 5 timeframes, ou seulement sur un/deux TF
choisis (ex. 15M et 1H, les plus utiles en pratique) pour ne pas surcharger le panneau ?

### 3. Order Blocks

Quand une structure (interne ou swing) casse (BOS/CHoCH), le script cherche — entre la bougie du
pivot cassé et la bougie de cassure — la bougie avec l'extrême le plus marqué (le plus haut pour
un OB baissier, le plus bas pour un OB haussier), **filtrée par volatilité** : si une bougie a un
range ≥ 2× la mesure de volatilité (ATR(200) par défaut), on utilise son *low* à la place de son
*high* (et vice versa) pour ne pas laisser une bougie anormalement grande fausser la zone. Cette
bougie devient l'Order Block. Il est retiré ("mitigé") dès que le prix revient le traverser
(close, ou high/low selon réglage — le script par défaut utilise high/low).

C'est le morceau le plus proche de ce qu'on a déjà : ça se branche directement sur les BOS/CHoCH
que `computeStructure` détecte déjà, il "suffit" d'ajouter la recherche de bougie-extrême +
filtre de volatilité + suivi de mitigation.

### 4. Fair Value Gaps

Détection sur 3 bougies : un FVG haussier existe si le low de la bougie courante est au-dessus du
high d'il y a 2 bougies, ET que la clôture d'il y a 1 bougie est aussi au-dessus (confirme un vrai
déplacement, pas juste une mèche), ET que l'ampleur du mouvement dépasse un seuil auto-calculé
(moyenne cumulée du delta de bougie en %, ×2). Symétrique pour un FVG baissier. Le script supporte
un FVG sur un **timeframe différent** du chart (`request.security`) — pas nécessaire pour aurum
puisqu'on calcule déjà chaque structure sur son propre timeframe nativement.

### 5. Equal Highs/Lows

Réutilise le mécanisme "leg" avec une longueur courte (3 par défaut) : un nouveau pivot est
"equal" au précédent s'il est à moins de `threshold × ATR` de distance (threshold par défaut 0.1).
Marque des zones de liquidité (stops probablement groupés).

### 6. Zones Premium / Discount / Equilibrium

Le plus simple des six : dérivé directement du swing high/low **trailing** (le plus haut/bas
courant depuis le dernier pivot, mis à jour à chaque bougie — pas juste le dernier pivot confirmé).
Premium = top 5% du range, Discount = bottom 5%, Equilibrium = bande 47.5%-52.5% (le milieu).
Quasi gratuit : on a déjà `swingHigh`/`swingLow` dans `StructureSnapshot`.

### 7. Niveaux MTF (high/low de la veille en D/W/M)

Fetch le high/low de la période précédente en Daily/Weekly/Monthly. `constants.ts` a déjà
`D_1`/`W_1`/`MN_1` dans `TRENDBAR_PERIODS` — juste un fetch supplémentaire (2 dernières bougies de
chaque période, on garde celle d'avant la courante).

### 8. Trendlines with Breaks — l'algo à part

Différent de tout le reste : à chaque nouveau pivot (symétrique, `length=14` par défaut — même
convention qu'aurum, cf. §1), une **pente** est calculée (3 méthodes au choix : ATR/`length`,
écart-type, ou régression linéaire — ATR par défaut) et la droite décroît/croît de cette pente à
chaque bougie tant qu'aucun nouveau pivot n'apparaît (donc une vraie diagonale, pas un niveau
horizontal comme BOS/CHoCH). Une cassure est détectée quand la clôture dépasse la droite
**projetée** (position actuelle moins `slope × length`, pour anticiper le point où la droite aurait
été si on l'avait tracée depuis le pivot). C'est le morceau le plus gros à porter — nouvel état à
maintenir en continu (pente + valeur courante de la droite), pas juste une comparaison ponctuelle.

## Séquencement proposé

1. **Order Blocks + Fair Value Gaps + Premium/Discount** — se branchent directement sur
   `computeStructure` existant, effort modéré, haute valeur.
2. **Niveaux MTF (D/W/M)** — quasi gratuit, un fetch de plus.
3. **Equal Highs/Lows** — nécessite le mécanisme "leg" (ou une variante avec l'existant, cf. §1).
4. **Structure interne** (si on la fait) — même remarque, dépend de la décision §1/§2.
5. **Trendlines & Break** — séparé, le plus gros morceau, en dernier.
6. **Panneau de suggestions discrétionnaires** — une fois tout ça en place, pour qu'il reflète la
   vraie méthode plutôt qu'une version partielle.

## Décisions à trancher avant de coder (résumé)

1. Méthode de pivot pour structure interne/swing/EQH-EQL : porter "leg" fidèlement, ou réutiliser
   `isPivotHigh`/`isPivotLow` déjà existant et testé ?
2. Structure interne : sur les 5 timeframes, ou seulement 1-2 en particulier ?
3. Longueurs par défaut à garder telles quelles (interne=5, swing=50, EQH/EQL=3) ou adaptées au
   multi-timeframe déjà en place (comme on l'a fait pour 5M/D1 dans `structure.ts`) ?
4. Où est-ce que tout ça s'affiche : extension du panneau STRUCTURE existant, ou un/plusieurs
   nouveaux panneaux dédiés (Order Blocks, FVG...) ?

## Notes techniques

- Chaque nouveau concept = un module pur dans `domain/`, testé comme `computeStructure` déjà
  l'est (bun:test, cas construits à la main comme dans `structure.test.ts`).
- Réutiliser l'infra de fetch déjà en place (`useStructure.ts`, `fetchHistory`) plutôt que d'en
  recréer une par concept.
- Le portage reste un portage de *comportement*, pas de code Pine — même principe que le script
  ALV déjà adapté (pas de mode repaint à gérer, on ne calcule que sur des bougies closes).
