"""Cuts CC0 recordings from OpenGameArt into the game's sound files (assets/sounds/*.ogg).

usage: python3 tools/cut_cc0_sounds.py <folder with the downloads> <out folder>

The downloads (all CC0, https://opengameart.org/content/<page>):
  fantozzis-footsteps-grasssand-stone  Fantozzi-footsteps.7z   (unpacked)  -> step, step_soft
  chunky-explosion                     Chunky Explosion.mp3                -> explosion
  amb-rain-loop-1                      amb_rain_loop_1.ogg                 -> rain
  helicopter-sfx                       heli.ogg                            -> heli
  door-open-door-close-set             qubodup-DoorSet.7z      (unpacked)  -> door, door_shut

Steps and doors: from the onset, a short fade at the end, peak levelled. Loops (rain,
helicopter): a steady stretch whose end is cross-faded into its start, so that it repeats
without a seam. Needs ffmpeg (with libvorbis) and numpy.
"""
import sys, subprocess, os, glob, numpy as np
SRC, OUT = sys.argv[1], sys.argv[2]
SR = 44100
os.makedirs(OUT, exist_ok=True)

def find(name):
    hits = glob.glob(os.path.join(SRC, '**', name), recursive=True)
    if not hits:
        sys.exit(f'missing {name} under {SRC}')
    return sorted(hits)[0]

def load(name):
    raw = subprocess.run(['ffmpeg', '-v', 'error', '-i', find(name), '-ac', '2', '-ar', str(SR),
                          '-f', 'f32le', '-'], capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.float32).reshape(-1, 2).copy()

def save(name, x, q=5):
    x = np.clip(x, -1, 1).astype(np.float32)
    p = subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'f32le', '-ar', str(SR), '-ac', '2',
                        '-i', '-', '-c:a', 'libvorbis', '-q:a', str(q), os.path.join(OUT, name + '.ogg')],
                       input=x.tobytes())
    assert p.returncode == 0, name
    print(f'{name}.ogg  {len(x)/SR:.2f} s')

def trim(x, rel=0.06, pre=0.004, tail_rel=0.02):
    """From just before the onset to where it has died away."""
    m = np.abs(x).max(1)
    i = int(np.argmax(m > m.max() * rel))
    loud = np.nonzero(m > m.max() * tail_rel)[0]
    return x[max(0, i - int(pre * SR)):min(len(x), loud[-1] + int(0.03 * SR))]

def fade(x, fin=0.002, fout=0.04):
    x = x.copy()
    a, b = int(fin * SR), min(int(fout * SR), len(x))
    x[:a] *= np.linspace(0, 1, a)[:, None]
    x[-b:] *= ((np.cos(np.linspace(0, np.pi, b)) + 1) / 2)[:, None]
    return x

def peak(x, p):
    return x * (p / max(np.abs(x).max(), 1e-9))

def rms(x, r):
    return x * (r / max(np.sqrt((x ** 2).mean()), 1e-9))

def loop(x, start, length, xf):
    """`length` seconds from `start`, the `xf` seconds after it cross-faded into the start."""
    a, n, k = int(start * SR), int(length * SR), int(xf * SR)
    body, tail = x[a:a + n].copy(), x[a + n:a + n + k]
    w = np.sqrt(np.linspace(0, 1, k))[:, None]          # equal power
    body[:k] = body[:k] * w + tail * np.sqrt(1 - w ** 2)
    return body

# footsteps: three left and three right of each kind
for kind, out in (('Stone', 'step'), ('Sand', 'step_soft')):
    k = 0
    for side in 'LR':
        for i in (1, 2, 3):
            k += 1
            x = fade(trim(load(f'Fantozzi-{kind}{side}{i}.ogg')), fout=0.05)
            save(out if k == 1 else f'{out}_{k}', peak(x, 0.7))

# the explosion: the blast and three seconds of its rumble
x = trim(load('Chunky Explosion.mp3'))[:int(3.2 * SR)]
save('explosion', peak(fade(x, fout=1.2), 0.78), q=4)

# doors: three of each
for kind, out, picks in (('Open', 'door', (1, 3, 5)), ('Close', 'door_shut', (1, 4, 7))):
    for k, i in enumerate(picks, 1):
        x = fade(trim(load(f'qubodup-Door{kind}{i:02d}.ogg'))[:int(1.6 * SR)], fout=0.15)
        save(out if k == 1 else f'{out}_{k}', peak(x, 0.7))

# rain all around: ten steady seconds
save('rain', rms(loop(load('amb_rain_loop_1.ogg'), 30.0, 10.0, 1.0), 0.11), q=4)

# the helicopter: four seconds of steady rotor
save('heli', peak(loop(load('heli.ogg'), 14.0, 4.0, 0.4), 0.65), q=4)
