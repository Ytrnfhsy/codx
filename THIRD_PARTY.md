# Сторонні ресурси

Код Toon Warfare — під ліцензією GNU GPL 3.0 або новішою (див. [`LICENSE`](LICENSE)).
Ця ліцензія **не** поширюється на перелічені нижче ресурси: кожен лишається під ліцензією
свого автора.

| Що | Де | Автор / джерело | Ліцензія |
|---|---|---|---|
| Моделі й анімації персонажів (Ultimate Animated Character Pack) | `assets/characters/characters.bin` | [Quaternius](https://quaternius.com) | CC0 1.0 (суспільне надбання) |
| Шрифти DejaVu Sans, DejaVu Sans Bold | `assets/fonts/` | [DejaVu fonts](https://dejavu-fonts.github.io/) (на основі Bitstream Vera) | вільна ліцензія DejaVu/Bitstream Vera, див. [`assets/fonts/LICENSE`](assets/fonts/LICENSE) |
| Кроки, двері, вибух, дощ, гелікоптер | `assets/sounds/` (`step*`, `door*`, `explosion`, `rain`, `heli`) | [OpenGameArt](https://opengameart.org), зокрема qubodup — перелік сторінок у [`assets/sounds/README.md`](assets/sounds/README.md#credits) | CC0 1.0 |
| Постріли й звуки перезарядки зброї | `assets/sounds/` (`gun_*`, `mag_*`, `bolt_*`) | записи з [Pixabay](https://pixabay.com) і [QuickSounds.com](https://quicksounds.com) | ліцензії цих сайтів (Pixabay Content License тощо): можна використовувати в грі, але **не** можна поширювати як окремі звукові файли чи збірки звуків |
| Рушій macroquad (з виправленням атласу шрифту) | `vendor/macroquad/` | [not-fl3/macroquad](https://github.com/not-fl3/macroquad) | MIT або Apache 2.0, див. `vendor/macroquad/LICENSE-*` |

Бібліотеки Rust, що підтягуються з crates.io (`Cargo.lock`), мають власні ліцензії — переважно
MIT, Apache 2.0 і подібні дозвільні, сумісні з GPL 3.0.

Решта — код, іконка (`assets/icon/`), синтезовані звуки й карти — створено для цієї гри й
поширюється під GPL 3.0 разом із кодом.
