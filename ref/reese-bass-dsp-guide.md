# Reese Bass: DSP Implementation Guide (From Scratch)

No plugins, no DAWs — just the signal-processing algorithms needed to generate and shape a Reese bass in code (C/C++, Python, JS/WebAudio, Rust, whatever). Everything below is expressed as math and pseudocode you can port directly.

---

## 1. Core Principle

A Reese is two (or more) detuned periodic oscillators summed together. Two waves at slightly different frequencies `f1` and `f2` produce a beating/phasing envelope at frequency `|f1 - f2|`, and because both are harmonically rich (sawtooth), every harmonic pair beats independently, producing a dense, swirling interference pattern rather than a single simple tremolo. As the fundamental pitch rises, `|f1 - f2|` rises too (if detune is expressed as a fixed ratio), so the beating audibly speeds up with pitch — this is the property that makes it a "real" Reese rather than a static two-oscillator patch.

---

## 2. Oscillator Generation

### 2.1 Naive sawtooth (phase accumulator)

```
phase = 0.0                  # runs 0..1
phase_inc = freq / sample_rate

def next_sample():
    global phase
    sample = 2.0 * phase - 1.0   # maps 0..1 -> -1..1
    phase += phase_inc
    if phase >= 1.0:
        phase -= 1.0
    return sample
```

This aliases badly at bass-register harmonics folded from higher voices when detuned/unison'd, so for anything used in production you want a band-limited version.

### 2.2 Band-limited sawtooth (PolyBLEP)

PolyBLEP corrects the discontinuity at the wrap point by subtracting a small polynomial correction near the edge.

```
def poly_blep(t, dt):
    # t = phase (0..1), dt = phase increment for this sample
    if t < dt:
        t /= dt
        return t + t - t*t - 1.0
    elif t > 1.0 - dt:
        t = (t - 1.0) / dt
        return t*t + t + t + 1.0
    else:
        return 0.0

def next_sample_blep():
    global phase
    dt = phase_inc
    saw = 2.0 * phase - 1.0
    saw -= poly_blep(phase, dt)
    phase += dt
    if phase >= 1.0:
        phase -= 1.0
    return saw
```

Use this for every oscillator voice in the Reese stack — with 7–9 detuned voices, naive-saw aliasing compounds fast.

### 2.3 Frequency from MIDI note

```
def note_to_freq(note, a4=440.0):
    return a4 * 2 ** ((note - 69) / 12.0)
```

---

## 3. Detuning Math

Detune is expressed in cents (1/100 of a semitone). Convert to a frequency multiplier:

```
def cents_to_ratio(cents):
    return 2 ** (cents / 1200.0)

f_detuned = f_base * cents_to_ratio(detune_cents)
```

For a classic two-oscillator Reese:

```
f1 = note_to_freq(note)
f2 = f1 * cents_to_ratio(+detune_cents)   # e.g. +8 cents
# optional third voice an octave down:
f3 = f1 * 0.5 * cents_to_ratio(-detune_cents)
```

Beat rate between two voices ≈ `|f1 - f2|` in Hz. Because this is computed as a ratio of `f1`, the beat rate scales with pitch automatically — this reproduces the "faster wobble at higher notes" property without any extra code.

---

## 4. Unison Stack (N-voice supersaw algorithm)

To generalize beyond two oscillators:

```
def make_unison_freqs(base_freq, voices, detune_cents):
    freqs = []
    pans = []
    for i in range(voices):
        if voices == 1:
            spread = 0.0
        else:
            spread = (2.0 * i / (voices - 1)) - 1.0   # -1..+1, symmetric
        cents = spread * detune_cents
        freqs.append(base_freq * cents_to_ratio(cents))
        pans.append(spread)   # use directly as pan position -1..+1
    return freqs, pans
```

Use an **odd** voice count (5, 7, 9) so one voice lands exactly at `spread = 0` (zero detune, center pan) — this keeps a phase-stable, mono-compatible anchor under the moving voices.

### Mixing/panning voices to stereo buffers

```
def mix_voice_to_stereo(sample, pan, out_l, out_r, i, voices):
    theta = (pan + 1.0) * (math.pi / 4.0)   # maps -1..1 -> 0..pi/2, equal-power law
    gain_l = math.cos(theta)
    gain_r = math.sin(theta)
    out_l[i] += sample * gain_l / math.sqrt(voices)
    out_r[i] += sample * gain_r / math.sqrt(voices)
```

Normalize by `1/sqrt(voices)` (power-preserving) rather than `1/voices` (over-attenuates) or no normalization (clips).

---

## 5. Envelope Generator (ADSR + pitch "bump")

```
class ADSR:
    def __init__(self, a, d, s, r, sr):
        self.a, self.d, self.s, self.r = a, d, s, r
        self.sr = sr
        self.stage = 'idle'
        self.level = 0.0

    def note_on(self):
        self.stage = 'attack'

    def note_off(self):
        self.stage = 'release'

    def process(self):
        if self.stage == 'attack':
            self.level += 1.0 / (self.a * self.sr)
            if self.level >= 1.0:
                self.level = 1.0
                self.stage = 'decay'
        elif self.stage == 'decay':
            self.level += (self.s - self.level) / (self.d * self.sr)
            if abs(self.level - self.s) < 1e-4:
                self.stage = 'sustain'
        elif self.stage == 'sustain':
            self.level = self.s
        elif self.stage == 'release':
            self.level += (0.0 - self.level) / (self.r * self.sr)
        return self.level
```

For the characteristic Reese "thump" that compensates its naturally slow perceived attack, run a **second**, much faster envelope (attack ~1ms, decay ~30–80ms, sustain 0) modulating pitch upward at note-on:

```
pitch_bump_env = ADSR(a=0.001, d=0.05, s=0.0, r=0.01, sr=sample_rate)
# each sample:
bump_semitones = pitch_bump_env.process() * bump_depth   # e.g. depth = 12
freq_mod_ratio = 2 ** (bump_semitones / 12.0)
f1_instant = f1 * freq_mod_ratio
f2_instant = f2 * freq_mod_ratio
```

---

## 6. LFO for Movement

```
class LFO:
    def __init__(self, freq, sr, shape='sine'):
        self.phase = 0.0
        self.inc = freq / sr
        self.shape = shape

    def process(self):
        p = self.phase
        self.phase += self.inc
        if self.phase >= 1.0:
            self.phase -= 1.0
        if self.shape == 'sine':
            return math.sin(2 * math.pi * p)
        elif self.shape == 'triangle':
            return 4 * abs(p - 0.5) - 1.0
```

Route LFO output to:
- **Filter cutoff:** `cutoff_hz = base_cutoff * (2 ** (lfo.process() * mod_depth_octaves))`
- **Detune amount:** `detune_cents_instant = base_detune + lfo.process() * detune_mod_depth`

Use a slow LFO (0.1–2 Hz) for evolving movement distinct from the fast, pitch-scaled inter-oscillator beating.

---

## 7. Filtering (Biquad / State-Variable Low-Pass)

RBJ (Robert Bristow-Johnson) cookbook biquad low-pass — standard, numerically stable, easy to implement from scratch:

```
def biquad_lpf_coeffs(cutoff_hz, q, sr):
    omega = 2 * math.pi * cutoff_hz / sr
    alpha = math.sin(omega) / (2 * q)
    cos_w = math.cos(omega)

    b0 = (1 - cos_w) / 2
    b1 = 1 - cos_w
    b2 = (1 - cos_w) / 2
    a0 = 1 + alpha
    a1 = -2 * cos_w
    a2 = 1 - alpha

    return (b0/a0, b1/a0, b2/a0, a1/a0, a2/a0)

class Biquad:
    def __init__(self):
        self.x1 = self.x2 = self.y1 = self.y2 = 0.0

    def process(self, x, coeffs):
        b0, b1, b2, a1, a2 = coeffs
        y = b0*x + b1*self.x1 + b2*self.x2 - a1*self.y1 - a2*self.y2
        self.x2, self.x1 = self.x1, x
        self.y2, self.y1 = self.y1, y
        return y
```

Recompute `coeffs` per-sample (or per-block, cheaper) if cutoff is modulated by the LFO/envelope.

For a more "analog" resonant character, a **state-variable filter (SVF)** (Chamberlin/TPT topology) is cheaper per-sample and self-stable under modulation:

```
class SVF:
    def __init__(self, sr):
        self.sr = sr
        self.low = self.band = 0.0

    def process(self, x, cutoff_hz, q):
        f = 2 * math.sin(math.pi * cutoff_hz / self.sr)
        fb = 1.0 / (q + q * (1.0 - f * f))
        self.low += f * self.band
        high = x - self.low - fb * self.band
        self.band += f * high
        return self.low   # low output; `high`, and (low+high) give HP/BP/notch variants
```

---

## 8. Waveshaping Distortion / Saturation

Pure per-sample nonlinear functions — no external tool needed.

```
def soft_clip_tanh(x, drive):
    return math.tanh(x * drive) / math.tanh(drive)

def soft_clip_cubic(x, drive):
    x = x * drive
    if x <= -1: return -2.0/3.0
    if x >= 1: return 2.0/3.0
    return x - (x**3) / 3.0

def arctan_saturate(x, drive):
    return (2.0 / math.pi) * math.atan(x * drive)
```

Apply distortion **before** the low-pass filter for a smoother, rounded top end, or **after** a resonant filter stage to make resonant peaks crunch rather than just ring — the two orderings are audibly different.

Oversample (2x–4x) around the nonlinearity to control aliasing from the new harmonics distortion generates:

```
def process_oversampled(x_block, nonlinear_fn, upsample_fir, downsample_fir):
    up = upsample_fir(x_block, factor=4)
    shaped = [nonlinear_fn(s) for s in up]
    return downsample_fir(shaped, factor=4)
```

(A simple polyphase or even a linear-interpolation upsampler + halfband FIR decimator is sufficient for a bass source.)

---

## 9. Multiband Split (Linkwitz-Riley Crossover)

To process low/mid/high independently, cascade two matched biquad LPF/HPF pairs (Linkwitz-Riley = two identical Butterworth stages in series, giving flat magnitude sum at the crossover):

```
def linkwitz_riley_split(x, crossover_hz, sr):
    lpf_coeffs = biquad_lpf_coeffs(crossover_hz, q=0.7071, sr=sr)
    hpf_coeffs = biquad_hpf_coeffs(crossover_hz, q=0.7071, sr=sr)

    low = biquad_apply_twice(x, lpf_coeffs)     # 2nd-order applied twice = 4th-order L-R
    high = biquad_apply_twice(x, hpf_coeffs)
    return low, high
```

`biquad_hpf_coeffs` uses the same RBJ derivation with:
```
b0 = (1 + cos_w) / 2
b1 = -(1 + cos_w)
b2 = (1 + cos_w) / 2
```
(a0, a1, a2 identical to the LPF case.)

Split at ~100–200 Hz: sum the low band's stereo signal to mono (see §11) and lightly saturate it; apply heavier saturation/modulation to the mid/high bands.

---

## 10. Sidechain Compression (Envelope Follower + Gain Computer)

```
class Compressor:
    def __init__(self, threshold_db, ratio, attack_ms, release_ms, sr):
        self.threshold_db = threshold_db
        self.ratio = ratio
        self.attack_coeff = math.exp(-1.0 / (attack_ms * 0.001 * sr))
        self.release_coeff = math.exp(-1.0 / (release_ms * 0.001 * sr))
        self.env_db = -100.0

    def process_gain(self, sidechain_sample):
        input_db = 20 * math.log10(abs(sidechain_sample) + 1e-9)
        if input_db > self.env_db:
            self.env_db = self.attack_coeff * self.env_db + (1 - self.attack_coeff) * input_db
        else:
            self.env_db = self.release_coeff * self.env_db + (1 - self.release_coeff) * input_db

        if self.env_db > self.threshold_db:
            over = self.env_db - self.threshold_db
            gain_reduction_db = over * (1.0 - 1.0/self.ratio)
        else:
            gain_reduction_db = 0.0
        return 10 ** (-gain_reduction_db / 20.0)
```

Feed the kick drum signal in as `sidechain_sample`, multiply Reese output by the returned gain each sample:

```
gain = compressor.process_gain(kick_signal[i])
reese_out[i] *= gain
```

Fast attack (1–5 ms), release timed to tempo (`release_ms ≈ (60000/bpm) * fraction_of_beat`) gives the classic pumping duck against the kick.

### "Hoover" retrigger variant

Instead of a smooth compressor, gate/retrigger the Reese's amplitude with a hard-edged rhythmic envelope synced to the kick pattern, so successive retriggered segments start out of phase with each other:

```
def hoover_gate(sample_index, bpm, sr, gate_length_ratio=0.5):
    samples_per_beat = sr * 60.0 / bpm
    pos_in_beat = (sample_index % samples_per_beat) / samples_per_beat
    return 1.0 if pos_in_beat < gate_length_ratio else 0.0
```

---

## 11. Stereo Width Control

### Mono-summing the low band
Bass information below ~100–150 Hz should generally be phase-coherent (mono) to avoid cancellation on mono playback and keep sub energy centered:

```
def mono_sum(l, r):
    m = (l + r) * 0.5
    return m, m
```

Apply this only to the low band output from §9's crossover split.

### Mid-Side (M/S) processing for the upper bands
```
def to_ms(l, r):
    mid = (l + r) * 0.5
    side = (l - r) * 0.5
    return mid, side

def from_ms(mid, side):
    l = mid + side
    r = mid - side
    return l, r
```
Scale `side` up (`side *= width_factor`, e.g. 1.2–1.5) before converting back, to widen only the mid/high bands while leaving the mono low band untouched.

### Simple chorus (modulated comb delay) for the "swimming" effect
```
class Chorus:
    def __init__(self, sr, max_delay_ms=20):
        self.buf = [0.0] * int(sr * max_delay_ms / 1000)
        self.write_idx = 0
        self.sr = sr
        self.lfo = LFO(freq=0.3, sr=sr, shape='sine')

    def process(self, x, depth_ms=5, center_ms=10, mix=0.3):
        delay_samples = (center_ms + self.lfo.process() * depth_ms) * self.sr / 1000.0
        read_idx = (self.write_idx - delay_samples) % len(self.buf)
        i0 = int(read_idx)
        frac = read_idx - i0
        i1 = (i0 + 1) % len(self.buf)
        delayed = self.buf[i0] * (1 - frac) + self.buf[i1] * frac  # linear interpolation

        self.buf[self.write_idx] = x
        self.write_idx = (self.write_idx + 1) % len(self.buf)
        return x * (1 - mix) + delayed * mix
```

---

## 12. Full Signal Chain (Assembly Order)

```
for each sample i:
    1. compute f1, f2 (+ optional f3) from note, detune math (§3), pitch-bump env (§5)
    2. generate PolyBLEP saw for each voice, or full unison stack (§2, §4)
    3. sum voices -> mono or stereo (pan per §4)
    4. apply amplitude ADSR (§5)
    5. modulate filter cutoff via LFO (§6), run through biquad/SVF (§7)
    6. waveshape distortion, oversampled (§8)
    7. split into low/mid/high bands (§9), mono-sum low band (§11)
    8. apply chorus/width processing to mid/high bands (§11)
    9. recombine bands
    10. apply sidechain compression against kick signal (§10)
    11. output
```

---

## 13. Validation / Authenticity Check

Sweep the patch across 2–3 octaves and measure the beat frequency between voices at each pitch (e.g. via FFT peak spacing or a zero-crossing counter on the summed signal's amplitude envelope). If beat rate does not scale proportionally with fundamental frequency, the detune is likely being applied as a fixed Hz offset rather than a cents/ratio offset (§3) — fix by ensuring detune multiplies frequency rather than adding a constant.
