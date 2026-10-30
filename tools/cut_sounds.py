"""Cuts the recorded sound pack into the game's sound files (assets/sounds/*.ogg).

usage: python3 tools/cut_sounds.py <pack folder with the .mp3 files> <out folder> <folder with gun_ak47*.wav>

Gunshots: each single shot from its onset, a cosine fade over the tail, then levelled to the
loudness of the synthesized guns (first 150 ms) with a soft limiter. Reload parts: cut
around their clicks, the hum/hiss between clicks gated away, peak -2 dB. Needs ffmpeg
(with libvorbis) and numpy.
"""
import sys, subprocess, numpy as np, os
SRC, OUT = sys.argv[1], sys.argv[2]
SR = 44100
os.makedirs(OUT, exist_ok=True)
cache = {}
def load(f):
    if f not in cache:
        raw = subprocess.run(['ffmpeg','-v','error','-i',os.path.join(SRC,f),'-ac','2','-ar',str(SR),'-f','f32le','-'],capture_output=True).stdout
        cache[f] = np.frombuffer(raw, dtype=np.float32).reshape(-1,2).copy()
    return cache[f]
def db(x): return 20*np.log10(max(x,1e-9))
def env_db(x, hop=220):
    m = np.abs(x).max(1); n = len(m)//hop
    e = np.sqrt((m[:n*hop].reshape(n,hop)**2).mean(1)); return 20*np.log10(e+1e-9)
def cut(f, a, b, pitch=1.0):
    x = load(f)[int(a*SR):int(b*SR)]
    if pitch != 1.0:   # resample: plays `pitch` times higher and shorter
        n = int(len(x)/pitch); t = np.arange(n)*pitch
        x = np.stack([np.interp(t, np.arange(len(x)), x[:,c]) for c in (0,1)],1).astype(np.float32)
    return x
def trim_onset(x, rel=0.08, pre=0.003):
    m = np.abs(x).max(1); i = int(np.argmax(m > m.max()*rel))
    return x[max(0, i-int(pre*SR)):]
def fades(x, fin=0.002, fade_from=None):
    x = x.copy(); n = len(x)
    k = int(fin*SR); x[:k] *= np.linspace(0,1,k)[:,None]
    s = int((fade_from if fade_from is not None else 0.7*n/SR)*SR)
    if s < n:
        r = np.linspace(0,1,n-s); x[s:] *= ((np.cos(r*np.pi)+1)/2)[:,None]
    return x
def gate(x, floor_db, hop=220, knee=8.0):
    """Downward expander: quiet stretches between clicks (hum, hiss) fade away."""
    e = env_db(x, hop); g = np.clip((e - floor_db)/knee, 0, 1)
    # smooth: fast open, slower close
    sm = np.zeros_like(g); v = 0
    for i, t in enumerate(g):
        v = t if t > v else v*0.85 + t*0.15; sm[i] = v
    gs = np.repeat(sm, hop); gs = np.pad(gs, (0, max(0,len(x)-len(gs))), 'edge')[:len(x)]
    return x*gs[:,None]
def soft_limit(x, knee=0.7):
    a = np.abs(x); over = a > knee
    y = x.copy()
    y[over] = np.sign(x[over])*(knee + (1-knee)*np.tanh((a[over]-knee)/(1-knee)))
    return y*0.97
def level(x, rms_db, win=0.15):
    """Loudness of the first 150 ms (mono mix) to rms_db, peaks softly limited."""
    for _ in range(4):
        r = db(np.sqrt((x[:int(win*SR)].mean(1)**2).mean()))
        x = soft_limit(x*10**((rms_db-r)/20))
    return x
def peak(x, p_db):
    return x*(10**(p_db/20)/np.abs(x).max())
def save(name, x):
    path = os.path.join(OUT, name + '.ogg')
    pcm = np.clip(x, -1, 1).astype(np.float32).tobytes()
    subprocess.run(['ffmpeg','-v','error','-y','-f','f32le','-ar',str(SR),'-ac','2','-i','-','-c:a','libvorbis','-q:a','5',path], input=pcm, check=True)
    print(f"{name:16s} {len(x)/SR:5.2f}s peak {db(np.abs(x).max()):5.1f} dB  {os.path.getsize(path)//1024} KB")

def variants(base, xs):
    for i, x in enumerate(xs):
        save(base if i == 0 else f"{base}_{i+1}", x)

def shot(f, at, length, fade_from, rms, pitch=1.0, rel=0.08):
    x = trim_onset(cut(f, max(0, at-0.08), at+length+0.1, pitch), rel)
    x = x[:int(length*SR)]
    return level(fades(x, fade_from=fade_from), rms)

# ---- gunshots
M4 = 'M4A1 Base A Close - QuickSounds.com.mp3'
variants('gun_m4', [shot(M4, t, 1.4, 0.6, -16) for t in (4.37, 9.38, 14.37, 19.37, 24.35)])
MP5 = 'MP5 Shot Close - QuickSounds.com.mp3'
variants('gun_mp5', [shot(MP5, t, 1.1, 0.45, -16) for t in (1.24, 6.25, 11.25, 16.25, 21.23)])
R700 = 'Remington 700 Shot Close - QuickSounds.com.mp3'
variants('gun_r700', [shot(R700, t, 2.4, 1.2, -15) for t in (0.47, 5.52, 10.53, 15.53, 20.54)])
# the M40A3 is a close cousin: the remaining takes, a touch deeper
variants('gun_m40', [shot(R700, t, 2.4, 1.2, -15, pitch=0.94) for t in (25.54, 30.55, 0.47)])
P90 = 'p90-sound-effect-2-351438.mp3'
variants('gun_p90', [gate(shot(P90, t, 0.9, 0.35, -16), -40) for t in (0.25, 1.27, 2.23)])
RPD = 'rpd-gun-shots-34210.mp3'
variants('gun_rpd', [shot(RPD, 1.36, 2.0, 0.7, -15, pitch=p) for p in (1.0, 1.035, 0.97)])
AK2 = 'AK-47 Automatic Rifle 2 - QuickSounds.com.mp3'
save('gun_ak47_3', shot(AK2, 0.36, 0.9, 0.45, -16))
save('gun_m9', shot('beretta-m9-shot-106671.mp3', 0.21, 1.2, 0.45, -16.5))
save('gun_usp', shot('usp-pistol-sfx-80490.mp3', 0.04, 1.2, 0.45, -16.5))
save('gun_deagle', shot('desert-eagle-gunshot-14622.mp3', 0.04, 0.84, 0.4, -16))

# the M249 recording is one short burst: the first (or second) shot's crack, then the
# burst's own ring-out after the last shot, so a single shot still decays naturally
M249 = 'light-machine-gun-m249-27686.mp3'
x = load(M249)
tail = x[int(0.60*SR):int(2.9*SR)]
outs = []
for s0 in (0.125, 0.192):
    hit = trim_onset(x[int((s0-0.01)*SR):int((s0+0.09)*SR)], 0.2)[:int(0.068*SR)]
    xf = int(0.03*SR); r = np.linspace(0,1,xf)[:,None]
    t = tail * (np.abs(hit[-xf:]).mean()/max(np.abs(tail[:xf]).mean(),1e-6))**0.5
    y = np.concatenate([hit[:-xf], hit[-xf:]*(1-r) + t[:xf]*r, t[xf:]])
    outs.append(level(fades(y, fade_from=0.6), -15))
variants('gun_m249', outs)

# ---- reload parts (per weapon)
def part(f, a, b, floor=-48, p=-2.0, fade_from=None):
    x = cut(f, a, b)
    x = gate(x, floor)
    x = fades(x, fin=0.003, fade_from=fade_from if fade_from is not None else (b-a)*0.85)
    return peak(x, p)
AKM = 'Insert Magazine Into AK-47 - QuickSounds.com.mp3'
for w in ('ak47', 'ak74u'):
    save(f'mag_in_{w}', part(AKM, 0.15, 0.70))
    save(f'bolt_{w}', part(AKM, 1.38, 2.80))
M5 = 'mp5-168858.mp3'
save('mag_out_mp5', part(M5, 0.0, 0.47))
variants('mag_in_mp5', [part(M5, 0.83, 1.50), part(M5, 1.62, 2.28)])
save('bolt_mp5', part(M5, 2.57, 3.40))
save('mag_out_p90', part(P90, 7.12, 7.62, floor=-36))
save('mag_in_p90', part(P90, 8.38, 8.85, floor=-36))
save('bolt_p90', part(P90, 9.45, 10.22, floor=-36))
LM = 'light-machine-gun-m249-39817.mp3'
save('mag_out_m249', part(LM, 1.22, 1.65))
save('mag_in_m249', part(LM, 1.88, 2.78))
variants('bolt_m249', [part(LM, 0.42, 1.05), part(LM, 2.85, 3.42)])
save('mag_out_rpd', part(RPD, 18.62, 19.20))
save('mag_in_rpd', part(RPD, 23.00, 23.55))
variants('bolt_rpd', [part(RPD, 25.15, 25.78), part(RPD, 24.25, 24.62)])
save('bolt_usp', part('45-93466.mp3', 0.26, 0.98))

# the AK-47 recordings already in the game (WAV, compiled in), to the same loudness
for n in ('gun_ak47', 'gun_ak47_2'):
    src = os.path.join(sys.argv[3], n + '.wav')
    raw = subprocess.run(['ffmpeg','-v','error','-i',src,'-ac','2','-ar',str(SR),'-f','f32le','-'],capture_output=True).stdout
    x = level(np.frombuffer(raw, dtype=np.float32).reshape(-1,2).copy(), -16)
    subprocess.run(['ffmpeg','-v','error','-y','-f','f32le','-ar',str(SR),'-ac','2','-i','-','-c:a','pcm_s16le',os.path.join(OUT, n + '.wav')], input=x.astype(np.float32).tobytes(), check=True)
    print(n, 'releveled')
