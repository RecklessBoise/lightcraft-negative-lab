# Negative Lab pour LightCraft

Conversion de négatifs couleur argentiques (C-41) en positifs, intégrée à
[LightCraft](https://github.com/storytold/lightcraft) sous forme de panneau natif. C'est un
équivalent **original et open source** des fonctions d'un outil comme Negative Lab Pro pour
Lightroom : aucun code, aucun profil ni aucune ressource de ce produit n'est utilisé.

> Ce dépôt est un fork de LightCraft. Tout le travail « Negative Lab » vit dans
> [`crates/film`](crates/film) (le moteur) et quelques points d'accroche dans le pipeline, le
> moteur de commandes et l'interface (liste plus bas).

## Installation

Prérequis : [Rust](https://rustup.rs) (1.90 ou plus récent).

```bash
git clone https://github.com/RecklessBoise/lightcraft-negative-lab.git
cd lightcraft-negative-lab
cargo run --release -p lightcraft
```

La langue de l'interface se règle dans les préférences de LightCraft (Français disponible).

## Utilisation

1. **Importez vos scans** : TIFF 16 bits de préférence (DNG et raw de copie à l'appareil
   fonctionnent aussi). Numérisez **sans correction automatique des couleurs**, idéalement avec un
   profil linéaire, et laissez un peu de film vierge (la bande entre les vues) dans le cadre.
2. Ouvrez le panneau **Negative Lab** : icône pellicule dans la barre d'outils de droite, ou
   *Fenêtre → Negative Lab*.
3. Choisissez la **pellicule** (Portra 400, LomoChrome Metropolis, LomoChrome Purple…) puis
   **Convertir le négatif** (ou `Alt+N`). Avec plusieurs photos sélectionnées,
   **Convertir les N sélectionnées** traite tout le rouleau, chaque vue mesurée séparément.
4. Affinez :
   - **Base du film** : la pipette mesure le masque orange sur la bande vierge entre deux vues
     (plus précis que l'estimation automatique). **Auto** revient à l'estimation.
   - **Neutraliser** (0–100) : à 100, les ombres et les hautes lumières les plus extrêmes sont
     rendues neutres, comme le fait un scanner de labo. À 0, seul le profil d'émulsion décide de
     l'équilibre des couleurs : c'est le plus fidèle à la pellicule *si* votre scanner voit le film
     comme les courbes publiées. Baissez ce curseur pour garder un coucher de soleil chaud ou la
     dominante voulue d'une LomoChrome.
   - **Séparation des couleurs** (0–200 %) : corrige la diaphonie des colorants, ce qui redonne la
     saturation qu'un capteur à large bande perd.
   - **Marge de bord** : la part de chaque bord exclue de la mesure (bande de film, porte-film).
     Augmentez-la si votre scan inclut beaucoup de bord.
   - **Exposition automatique** : cale l'exposition de chaque vue sur son propre contenu.
   - **Positif** : balance des blancs, exposition, contraste, hautes lumières, ombres, blancs,
     noirs, vibrance et saturation, appliqués *après* la conversion. Tous les autres outils de
     LightCraft (courbes, étalonnage, masques…) fonctionnent aussi normalement sur le positif.
5. **Taille du panneau** : boutons **S / M / L / XL**, ou faites glisser le bord gauche du panneau.
   La largeur est mémorisée.

### Fidélité maximale : calibration par charte

Les profils intégrés sont des points de départ (voir plus bas). Pour reproduire le plus fidèlement
*votre* pellicule, *votre* labo et *votre* scanner :

1. Photographiez une charte de couleurs de 24 patchs (disposition ColorChecker Classic) sur le
   rouleau, en lumière du jour.
2. Ouvrez cette vue, **Calibrer avec une charte…**, puis cliquez au centre des 4 patchs de coin
   dans l'ordre indiqué : haut gauche (peau foncée), haut droite (vert bleuté), bas droite (noir),
   bas gauche (blanc).
3. LightCraft ajuste un profil mesuré (matrice 3×3 + décalages) et affiche l'erreur résiduelle en
   IL. Copiez ensuite les réglages (`Cmd+Shift+C` / `Cmd+Shift+V`, groupe *Negative Lab*) sur les
   autres vues du rouleau.

## Le modèle colorimétrique

Un négatif enregistre une **transmittance** par canal. Le film est linéaire en **densité**, c'est
donc là que se fait le calcul :

1. **Base** : `Dn = log10(T_base / T)`. Diviser par la base supprime à la fois le masque orange et
   la couleur de la source lumineuse du scanner.
2. **Canaux du scan** : LightCraft décode les fichiers dans son espace de travail (Rec.2020), ce qui
   mélange les canaux. Les densités sont donc calculées dans les canaux d'origine du scan
   (primaires Rec.709/sRGB, celles des fichiers de scanner), puis le positif revient en Rec.2020.
3. **Séparation** : `A = M⁻¹ · Dn` corrige la diaphonie des colorants cyan, magenta et jaune.
4. **Inversion** : chaque couche repasse de la densité au log de l'exposition par sa pente (gamma)
   et sa densité du gris moyen, ou, avec *Neutraliser*, en alignant les extrémités claires et
   denses de l'image. L'inversion est **linéaire en densité** : le pied et l'épaule propres à chaque
   film restent dans l'image, comme sur un tirage (ombres douces de la Portra, noirs bouchés de la
   Metropolis).
5. **Lumière** : `E = 0,18 · 10^(x + décalage)`, en lumière de scène linéaire. Toute la chaîne de
   développement de LightCraft (tonalité, couleur, export) s'applique ensuite comme sur un raw.

Les statistiques de l'image (base, extrémités, médiane) sont mesurées sur la source entière et
**enregistrées avec la retouche** : l'aperçu, la vignette, le zoom et l'export convertissent à
l'identique.

## Profils d'émulsion : ce qu'ils valent

| Pellicule | Source des données |
|---|---|
| Kodak Portra 160 / 400 / 800, Ektar 100, Gold 200, UltraMax 400 | Fiches techniques Kodak publiques (E-4051, E-4050, E-4040, E-4046, E-7022, E-7023), courbes Status M relevées **approximativement** |
| Fujicolor 200, CineStill 800T, Lomography Color Negative 400 | Estimations d'après des scans typiques |
| **LomoChrome Metropolis** | Lomography ne publie pas de courbes : **estimation** d'après la description du fabricant (couleurs désaturées, contraste fort) et des scans de labo |
| **LomoChrome Purple** | Lomography ne publie pas de courbes : **estimation**. Le virage de couleurs (verts → violets, bleus → vert sarcelle) est physiquement inscrit dans les couches du film ; une inversion correcte le restitue sans le simuler |

En clair : les profils Kodak suivent des données publiées, les profils Lomography sont des
estimations. Ce sont des points de départ, pas des mesures. **La calibration par charte est le
seul moyen d'atteindre une fidélité mesurée** : sans elle, aucun outil ne peut garantir la
colorimétrie exacte d'une émulsion, car le résultat dépend aussi du développement et du scanner.

## Développeurs

| Élément | Emplacement |
|---|---|
| Moteur (analyse, inversion, émulsions, calibration) | [`crates/film`](crates/film) |
| Réglages `film` (sérialisés seulement s'ils sont utilisés) | `crates/develop/src/settings.rs` (`Film`) |
| Curseurs `film.neutralize`, `film.separation`, `film.border` | `crates/develop/src/controls.rs` |
| Étape du pipeline (avant la balance des blancs) | `crates/pipeline/src/film.rs` |
| Commandes `film.*` (UI, CLI, MCP) | `crates/engine/src/cmd/film.rs` |
| Panneau | `crates/ui-egui/src/panels/negative_lab.rs` |

Commandes : `film.emulsions`, `film.convert`, `film.set`, `film.analyze`, `film.pickBase`,
`film.resetBase`, `film.calibrate`, `film.clearCalibration`. Exemple en ligne de commande :

```bash
echo '{"film":{"enabled":true,"emulsion":"kodak.portra400"}}' > film.json
cargo run --release -p lightcraft-cli -- render scan.tif -o positif.jpg --settings film.json
```

Fabriquer un négatif de test à partir d'une image positive :

```bash
cargo run -p lightcraft-film --example make_negative -- positif.jpg negatif.tif lomography.purple
```

Tests : `cargo test -p lightcraft-film -p lightcraft-pipeline -p lightcraft-engine film`.

## Licence

Comme LightCraft : MIT ou Apache-2.0, au choix. Kodak, Portra, Ektar, Fujicolor, CineStill,
Lomography, LomoChrome et Negative Lab Pro sont des marques de leurs propriétaires respectifs,
citées uniquement pour identifier des produits.
