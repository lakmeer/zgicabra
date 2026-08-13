
# TODO

- Filter audibly snaps back to neutral when note released
  - Still?
- Compressor feedback?
- Performance audit
- Switch GUI framework
  - raylib
  - gpui
- Play demo sequence
  - C3(1.5) F#2(1.5) F2(5), G3(1.5) C#3(1.5) C2(5)

- Gens
  - Basic
    - sin/tri/saw/sq select
    - tanh saturation
      - range protection
    - multivoice
  - Vital
  - SamplePlayer
  - Experiment with waveshapers

- FX
  - Nam
    - Test boost param (input volume affect on nam model output)
    - Add crossover param
  - OTT crusher 
    - Current crusher doesnt appear to be multiband
    - expose per-band thresholds as params
    - scale all bands attack and release together as 'time' param
    - single 'depth' param scales all ratios and thresholds
