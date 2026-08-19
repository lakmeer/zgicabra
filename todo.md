
# TODO

- Wait for hydra docking on startup?
- Fade master when width is negative
- Clamp master when wands are docked
- Filter audibly snaps back to neutral when note released
  - Still?
- Add DeltaEvent::NoteRetrig -> voices can decide what it means
- Is it appropriate to vendor NAM model files? Check tone3000 license info
- Reintroduce T3K IR but as finishing stage, replace final NAM pair
- Map DeltaEvent::NoteRetrig in Voices
- Audio connection drops after a long time idle; investigate
- When consuming DeltaEvent::VoiceChange(voice), use the actual voice
  value to determine index of selected voice, so that there is one place
  to rearrange the voice list (zgicabra.rs)
- Release note at trigger zenith

- Voices
  - Macro potential?
  - Basic
    - sin/tri/saw/sq select
    - multivoice
    - use to test midi tweaking/persistence
  - Growl
    - Needs dedicated sub
      - Power 5?
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
    - Kick sample
    - Bass sample

- FX
  - OTT crusher 
    - Current crusher doesnt appear to be multiband
    - expose per-band thresholds as params
    - scale all bands attack and release together as 'time' param
    - single 'depth' param scales all ratios and thresholds
  - Chorus/phasers
  - Waveshapers
  - Bitcrushers

- Panel
  - Await hardware delivery
  - Portrait mode?

- System
  - KMSCON
    - Works well, requires Terminus font to render TUI correctly:

      fonts.packages = with pkgs; [
        terminus_font_ttf
      ];

    - Specify font as "Terminus (TTF)" exactly
  - 6 NAM FF passes might be too much - test in bare env
  - Measured time-to-first-note: 
  - Strip out more default services

