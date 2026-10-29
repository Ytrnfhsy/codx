# Sounds

Every sound in the game is synthesized at start-up, so the game needs no audio files.
Any of them can be replaced with a real recording: put a `.ogg` or `.wav` file (mono or
stereo, any sample rate) with the right name into this folder, next to the game
(`<game folder>/assets/sounds/`) or in the folder the game is started from. Extra
variants that are picked at random go into `<name>_2.ogg`, `<name>_3.ogg`, … (up to 8).

Only use recordings you are allowed to ship with the game, for example CC0 packs.

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
| `hit` | hitmarker tick |
| `hit_head` | headshot ding |
| `kill` | kill confirmation |
| `step` | footstep |
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

`cargo test -p toon_client dump_sounds` with `TW_SOUND_DUMP=<folder>` writes the
synthesized versions as WAV files, handy for comparing.
