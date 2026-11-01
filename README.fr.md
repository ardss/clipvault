# ClipVault

[![Release](https://img.shields.io/github/v/release/ardss/clipvault)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/ardss/clipvault/ci.yml?label=CI)](../../actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ardss/clipvault)](LICENSE)

**Français** | [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Deutsch](README.de.md)

Le Win+V amélioré — un gestionnaire d’historique du presse-papiers Windows propre et minimaliste.

Pressez `Alt+V` pour invoquer le panneau → recherchez → cliquez pour recoller là d’où vous venez. 100 % local : zéro réseau, zéro compte, zéro télémétrie.

## Fonctionnalités

- **Historique durable** : texte, images, fichiers, texte enrichi (formats web/Word), liens — survit au redémarrage, rétention configurable
- **Recherche** : filtrage instantané par mot-clé (chinois et anglais)
- **Épingler** : gardez les entrées fréquentes en haut ; exclues du nettoyage
- **Filtres** : tout / texte / images / liens / fichiers / épinglés
- **Recoller** : le panneau ne vole jamais le focus de votre travail ; cliquez sur une entrée et elle atterrit où vous étiez
- **Aperçu au survol** : le texte tronqué et les miniatures d’images s’agrandissent dans un aperçu latéral (défilable, sélectionnable, partiellement copiable)
- **Clavier d’abord** : `↑↓` naviguer, `Entrée` coller, `Suppr` supprimer, `Échap` fermer
- **Thème sombre et clair, interface en 7 langues** (fr/en/zh/ja/ko/es/de) : se change dans le panneau, mémorisé
- **Statistiques** : totaux / catégories / tendance sur 7 jours / top 5 des collages
- **Démarrage automatique optionnel**, icône de la barre des tâches, hauteur du panneau réglable et mémorisée
- **Suspendre la capture** (un clic dans la barre des tâches, reprise automatique optionnelle) et **vider tout l’historique** en un clic chacun
- **Interception de confidentialité, tout en opt-in** : filtre de mots sensibles (avec interrupteur), exclusion d’applications, interrupteur de capture d’images ; les copies marquées par les gestionnaires de mots de passe ne sont jamais enregistrées (standard Windows)
- **Raccourci d’invocation configurable** (ré-enregistrement sûr en cas de conflit)
- **Texte surdimensionné** (>256 Ko) déchargé vers un fichier local avec aperçu en ligne — rien n’est perdu en silence

## Confidentialité

- 100 % local. Les données vivent dans `%APPDATA%\com.clipvault.app\` (SQLite WAL + fichiers PNG)
- Les copies des gestionnaires de mots de passe ne sont jamais enregistrées (les marqueurs sensibles du presse-papiers sont respectés)
- Filtre de mots sensibles défini par l’utilisateur
- Aucun code réseau dans le binaire. Détails dans [SECURITY.md](SECURITY.md)

## Installation

Prenez un installateur depuis [Releases](../../releases), ou compilez depuis les sources :

```bash
npm install
npx tauri build        # sortie dans src-tauri/target/release/bundle/
```

## Compilation

Tauri 2 + Vue 3 + Rust (rusqlite / couche Win32 écrite à la main).

> **Remarque :** compilez toujours les versions avec `npx tauri build`. Un `cargo build --release`
> nu laisse la configuration de développement active et la fenêtre tentera de charger l’UI
> depuis le serveur de dev vite (voir [docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md)).

Mode développement :

```bash
npx vite --port 5173        # terminal 1
npx tauri dev               # terminal 2
```

Tests : `cd src-tauri && cargo test` (aller-retour presse-papiers / décodage DIB / analyse DROPFILES)

## Tests et acceptation

Voir [docs/TESTING.md](docs/TESTING.md) et [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

## Notes techniques

- Compatibilité d’écriture du presse-papiers : les images sont proposées à la fois en CF_DIB standard (bottom-up) et au format enregistré « PNG » — les applications basées sur Chromium (Chrome/VS Code/Electron) comme les applications GDI les lisent correctement ; le décodage DIB prend en charge les masques de canaux BI_BITFIELDS (outils de capture d’écran)
- La frappe de collage est émise par un sous-processus injecteur à la demande (auto-réparateur) (dans certains contextes, les touches synthétiques du même processus sont avalées)
- Chien de garde des battements IPC de WebView : le panneau détecte un canal IPC mort et se recharge (vérification JS toutes les 5 s, repli natif sous 60 s)
- Toutes les données vivent dans `%APPDATA%\com.clipvault.app\`

## Licence

[MIT](LICENSE)
