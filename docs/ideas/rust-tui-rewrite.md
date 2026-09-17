# Idée : refonte Aurum en Rust (ratatui)

Note pour plus tard, pas un plan validé.

## Pitch

Repartir de zéro en Rust + [ratatui](https://ratatui.rs) au lieu de TS/Bun/OpenTUI. Objectif : un terminal de trading cTrader assez solide pour être **shippé en production** et **open source**, pas juste un projet perso.

## Ce qui change par rapport à Aurum actuel

- **Un seul symbole** : US100 uniquement, pas de multi-symboles. Simplifie l'UI, le risk management, et le code — on optimise pour un seul instrument au lieu de généraliser.
- **Rust + ratatui** au lieu de TS/Bun/OpenTUI — perf, robustesse, binaire unique distribuable.
- **Open source** dès le départ (licence à choisir), donc :
  - pas de secrets/tokens en dur, config claire (cf. approche actuelle : token chiffré, pas de `.env` obligatoire)
  - doc d'installation et de contribution soignée
  - tests, CI

## Bonnes pratiques de trading à intégrer

- Risk management strict : taille de position calculée depuis le risque en % de l'équity (comme aujourd'hui), jamais de saisie de lot brute par défaut
- SL/TP obligatoires ou dérivés de l'ATR — pas de position sans stop
- Limites de risque : max drawdown journalier, max pertes consécutives, coupe-circuit automatique
- Filtre calendrier économique (garder l'équivalent de `src/news`) pour éviter de trader pendant les news à fort impact sur US100
- Journal de trades intégré (log structuré : entrée, sortie, raison, R multiple) pour permettre le post-mortem
- Mode démo forcé par défaut, confirmation explicite pour passer en compte réel

## Pourquoi une note et pas un chantier maintenant

Aurum actuel (TS/Bun/OpenTUI, multi-symboles, privé) reste la base de travail courante. Cette refonte est une piste à explorer plus tard, pas une réécriture immédiate.
