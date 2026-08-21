# fundsp AudioNodes (https://github.com/SamiPerttu/fundsp)

Generators
  brown                 - Brownian noise source
  dsf_saw / dsf_saw_r   - Discrete summation formula sawtooth oscillator
  dsf_square / _r       - Discrete summation formula square oscillator
  hammond / hammond_hz  - Hammond organ-like oscillator
  impulse               - Multichannel impulse signal
  mls / mls_bits        - Maximum length sequence noise
  noise / white         - White noise source
  organ / organ_hz      - Organ wave oscillator
  pink                  - Pink noise source
  poly_pulse(_hz)       - PolyBLEP pulse wave
  poly_saw(_hz)         - PolyBLEP sawtooth wave
  poly_square(_hz)      - PolyBLEP square wave
  pulse                 - Bandlimited pulse wave
  ramp / ramp_hz        - Non-bandlimited sawtooth ramp
  saw / saw_hz          - Bandlimited sawtooth wave
  sine / sine_hz        - Sine oscillator
  soft_saw(_hz)         - Soft sawtooth oscillator
  square / square_hz    - Bandlimited square wave
  triangle / triangle_hz- Bandlimited triangle wave
  zero / multizero      - Silence signal
  constant / dc         - Constant signal value

Linear Filters
  allpass(_hz/_q)       - 2nd order allpass filter
  allpole / allpole_delay - 1st order allpass filter
  bandpass(_hz/_q)      - 2nd order bandpass filter
  bell(_hz/_q)          - Peaking/bell equalizer filter
  biquad                - Arbitrary biquad filter with coefficients
  butterpass(_hz)       - Butterworth lowpass filter
  dcblock(_hz)          - DC blocking filter
  highpass(_hz/_q)      - 2nd order highpass filter
  highpole(_hz)         - 1st order highpass filter
  highshelf(_hz/_q)     - High shelf equalizer
  lowpass(_hz/_q)       - 2nd order lowpass filter
  lowpole(_hz)          - 1st order lowpass filter
  lowshelf(_hz/_q)      - Low shelf equalizer
  morph(_hz)            - Morphing filter (lowpass/peak/highpass)
  notch(_hz/_q)         - Notch filter
  peak(_hz/_q)          - Peaking filter
  pinkpass              - Pink noise shaping filter
  resonator(_hz)        - Constant-gain bandpass resonator
  allnest(_c)           - Nested allpass filter
  fir                   - FIR filter with specified weights
  fir3                  - Symmetric 3-point FIR filter

Nonlinear Filters
  bandrez(_hz/_q)       - Resonant bandpass filter
  dbell(_hz)            - Dirty biquad bell equalizer
  dhighpass(_hz)        - Dirty biquad highpass
  dlowpass(_hz)         - Dirty biquad lowpass
  dresonator(_hz)       - Dirty biquad resonator
  fbell(_hz)            - Feedback biquad bell equalizer
  fhighpass(_hz)        - Feedback biquad highpass
  flowpass(_hz)         - Feedback biquad lowpass
  fresonator(_hz)       - Feedback biquad resonator
  lowrez(_hz/_q)        - Resonant lowpass filter
  moog(_hz/_q)          - Moog ladder lowpass filter

Delay & Time Effects
  delay                 - Delay by specified time
  tap / tap_linear      - Tapped delay with interpolation
  multitap(_linear)     - Multi-tap delay line
  tick / multitick      - Single sample delay
  flanger               - Flanging effect
  phaser                - Phaser effect

Dynamics & Envelopes
  adsr_live             - ADSR envelope with live control
  afollow               - Asymmetric smoothing filter
  follow                - Smoothing filter with response time
  limiter / limiter_stereo - Look-ahead limiter
  declick(_s)           - Fade-in declick

Reverb & Spatial
  reverb_stereo         - FDN stereo reverb
  reverb2_stereo        - Hybrid FDN stereo reverb
  reverb3_stereo        - Allpass loop stereo reverb
  pan                   - Fixed pan to stereo
  panner                - Dynamic mono-to-stereo panner
  rotate                - Stereo rotation with gain

Special Processing
  resynth               - Frequency domain resynthesis
  convolve              - Convolution filter
  pluck                 - Karplus-Strong plucked string
  shape / shape_fn      - Waveshaper distortion
  clip / clip_to        - Signal clipping
  meter                 - Signal metering
  monitor               - Monitoring pass-through
  hold(_hz)             - Sample-and-hold

Oscillator Modulation
  lorenz                - Lorenz system oscillator
  rossler               - Rössler system oscillator
  envelope / lfo        - Time-varying control
  envelope2 / lfo2      - Input-dependent control
  envelope3 / lfo3      - 2-input dependent control
  envelope_in / lfo_in  - Frame-based control

Signal Routing & Combination
  pass / multipass      - Pass-through signal
  sink / multisink      - Consume signal
  split / multisplit    - Split to multichannel
  join / multijoin      - Join multichannel
  reverse               - Reverse channel order
  add / sub / mul       - Arithmetic operations
  product / sum         - Multiply/sum two nodes
  pipe(i/f)             - Serial chaining
  branch(i/f)           - Parallel branching
  bus(i/f)              - Signal busing
  stack(i/f)            - Parallel stacking
  thru                  - Pass-through with parameter adjustment

Wave & Sample Playback
  playwave(_at)         - Play back wave data
  resample              - Resample generator at variable speed
  resample_fir          - FIR-based sinc resampling

Control & Feedback
  feedback / feedback2  - Single-sample feedback loop
  fdn / fdn2            - Feedback Delay Network
  listen                - Setting listener wrapper
  update                - Update node with interval
  var / var_fn          - Shared variable output
  timer                 - Stream time tracking
  oversample            - 2x oversampling
  biquad_bank           - SIMD-accelerated biquad bank
  chorus                - Chorus effect
  map                   - Custom channel mapping
  unit                  - Convert AudioUnit to AudioNode

