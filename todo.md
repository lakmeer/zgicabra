
# TODO

- Filter audibly snaps back to neutral then note released
- Remove def.lo.hi from matrix
- Can't hear FX chain
- Check curve blending
- Compressor feedback?
- Performance audit

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
