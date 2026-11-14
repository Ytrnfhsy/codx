# Sounds

Every sound in the game is synthesized at start-up, so the game needs no audio files.
Any of them can be replaced with a real recording: put a `.ogg` or `.wav` file (mono or
stereo, any sample rate) with the right name into this folder, next to the game
(`<game folder>/assets/sounds/`) or in the folder the game is started from. Extra
variants that are picked at random go into `<name>_2.ogg`, `<name>_3.ogg`, … (up to 8).

Only use recordings you are allowed to ship with the game, for example CC0 packs.

Built in (compiled into the game; files with the same names placed next to the game
replace them): recorded shots for the M4A1 (5 takes), AK-47 (3), MP5 (5), P90 (3), M249 (2),
RPD (3), R700 (5), M40A3 (3, the R700 takes a touch deeper), M9, USP .45 and Desert Eagle,
and the weapons' own reload sounds listed below; footsteps on hard floors and on soft
ground (6 takes each), the explosion, doors opening and shutting (3 takes each), the rain
and the helicopter. The G3, AK-74u and both shotguns, and every other sound, are still
synthesized. `tools/cut_sounds.py` rebuilds the gun files from the original pack (free
recordings from Pixabay and QuickSounds.com); `tools/cut_cc0_sounds.py` the rest from the
CC0 recordings credited at the end of this file.

Reload sounds can be given per weapon: `mag_out_<gun>`, `mag_in_<gun>`, `bolt_<gun>` (the
gun names as in the `gun_*` files). A weapon without its own falls back to `mag_out`,
`mag_in`, `bolt`. Built in: AK-47 and AK-74u (magazine in, charging handle), MP5 (all three,
two magazine-in takes), P90 (all three), M249 and RPD (all three, two bolt takes), USP
(slide).

| File | Sound |
|---|---|
| `gun_m4` | M4A1 shot (close) |
| `gun_ak47` | AK-47 shot |
| `gun_g3` | G3 shot |
| `gun_mp5` | MP5 shot |
| `gun_p90` | P90 shot |
| `gun_ak74u` | AK-74u shot |
| `gun_m249` | M249 shot |
| `gun_rpd` | RPD shot |
| `gun_w1200` | W1200 shotgun shot |
| `gun_m1014` | M1014 shotgun shot |
| `gun_m40` | M40A3 shot |
| `gun_r700` | R700 shot |
| `gun_m9` | M9 shot |
| `gun_usp` | USP .45 shot |
| `gun_deagle` | Desert Eagle shot |
| `heli_gun` | helicopter door gun |
| `far` | any gunfire far away (muffled, mostly echo) |
| `explosion` | grenade / airstrike / bomb |
| `smoke` | smoke grenade popping and hissing |
| `hit` | hitmarker tick |
| `hit_head` | headshot ding |
| `kill` | kill confirmation |
| `step` | footstep on a hard floor |
| `step_soft` | footstep on earth, grass or sand (big maps); `step` without it |
| `land` | landing after a jump |
| `mag_out` | magazine out |
| `mag_in` | magazine in |
| `bolt` | charging handle / bolt |
| `pump` | shotgun pump |
| `shell_in` | shotgun shell loaded |
| `shell` | brass casing on the floor |
| `raise` | weapon raised |
| `empty` | dry fire click |
| `knife` | knife swing |
| `knife_hit` | knife hit |
| `flesh` | bullet hitting a body |
| `whiz` | bullet flying past |
| `beep` | bomb beep |
| `jet` | jet flying over |
| `click` | menu click |
| `streak` | killstreak ready |
| `hurt` | taking damage |
| `bomb` | bomb planted / defused jingle |
| `heli` | helicopter rotor (looped: must loop seamlessly) |
| `drone` | drone rotors (looped) |
| `rain` | rain all around (looped) |
| `door`, `door_shut` | a door opening, shutting; a click without them |

`cargo test -p toon_client dump_sounds` with `TW_SOUND_DUMP=<folder>` writes the
synthesized versions as WAV files, handy for comparing.

## Credits

Recordings from [OpenGameArt](https://opengameart.org), all released as **CC0** (public
domain) on their pages; cut and levelled by `tools/cut_cc0_sounds.py`:

| Sounds | Source page |
|---|---|
| `step*`, `step_soft*` | `/content/fantozzis-footsteps-grasssand-stone` (Fantozzi's Footsteps, by qubodup) |
| `door*`, `door_shut*` | `/content/door-open-door-close-set` (by qubodup) |
| `explosion` | `/content/chunky-explosion` |
| `rain` | `/content/amb-rain-loop-1` |
| `heli` | `/content/helicopter-sfx` |
