
# TODO

- Filter audibly snaps back to neutral when note released
  - Still?
- Compressor feedback?
- Performance audit
- Switch GUI framework
  - raylib
  - gpui

- Voices
  - Basic
    - sin/tri/saw/sq select
    - multivoice
  - Growl
    - Multiband distortion
      - low: `bass`
      - hi: `mesa`
    - Phaser ^ width
  - Reese
    - Octave layers ^ width
    - vel ^ width_param
    - lfo_rate ^ width
  - SamplePlayer
  - Experiment with waveshapers

- FX
  - OTT crusher 
    - Current crusher doesnt appear to be multiband
    - expose per-band thresholds as params
    - scale all bands attack and release together as 'time' param
    - single 'depth' param scales all ratios and thresholds
