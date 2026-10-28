# Characters

`characters.bin` holds the soldier models and their walk/run/idle animations from the
**Ultimate Animated Character Pack** by [Quaternius](https://quaternius.com), released under
CC0 1.0 (public domain) — thank you, Quaternius!

The file is generated from the pack's glTF files:

```
python3 -I tools/convert_characters.py <pack>/glTF assets/characters/characters.bin
```

The script keeps the models and clips listed at its top, welds duplicate vertices and gives
the bare feet a boot material. The game loads it in `crates/client/src/character.rs`.
