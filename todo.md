
# TODO

- Wait for hydra on startup
- Fix MIDI CC mapping
- Fade master when width is negative
- Clamp master when wands are docked
- Filter audibly snaps back to neutral when note released
  - Still?
- Add DeltaEvent::NoteRetrig -> voices can decide what it means
- Is it appropriate to vendor NAM model files? Check tone3000 license info
- Reintroduce T3K IR but as finishing stage, replace final NAM pair
- Map DeltaEvent::NoteRetrig in Voices

- Voices
  - Basic
    - sin/tri/saw/sq select
    - multivoice
  - Growl
    - Needs dedicated sub
    - Multiband distortion
      - low: `bass`
      - hi: `mesa`
    - Phaser ^ width
  - Reese
    - Octave layers ^ width
    - vel ^ width_param
    - lfo_rate ^ pitch
    - drive noy audible
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
  - 6 NAMs might be too much - test in bare env
