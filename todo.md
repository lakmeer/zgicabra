
# TODO

- Filter audibly snaps back to neutral when note released
  - Still?
- Switch GUI framework
  - raylib
  - gpui
- Performance audit
- Static linking audit

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

- Panel
  - Simpler stack than imgui?
    - Can we do it without a compositor at all? raw FB?
      - Probably need it for raw boot mode anyway
  - Test KMSDRM mode in an alternate tty?

- System
  - KMSCON
    - Works well, requires Terminus font to render TUI correctly:

      fonts.packages = with pkgs; [
        terminus_font_ttf
      ];

    - Specify font as "Terminus (TTF)" exactly

