
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
  - Await hardware
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


## System setup

[X] Review `sys/config.nix` and the diff above.
[X] `nixed` (`sudo nvim /etc/nixos/configuration.nix`) → make the one
   addition and six removals → save.
[X] `sudo nixos-rebuild switch`.
[X] Reboot with no keyboard attached → should land on the KMSCON TUI
   running `zgicabra` on vt1, no login prompt, no manual steps.
[X] Reboot with a keyboard attached → select the `dev` entry at the
   GRUB menu within the timeout → should land in X11
[ ] From dev mode, run `bin/perform` (from the repo root) → KMSCON+zgicabra
   should come up on vt3 without disturbing the X session on vt1.
[ ] `sudo systemctl kill -s SIGTERM zgicabra` mid-session (from another vt
   or SSH) → service should restart (`Restart=on-failure`) and the voice/
   params selected beforehand should come back.
[ ] Cycle voices via the wand trigger with no MIDI controller attached →
   confirm `config/selected` updates in the repo each time.
[ ] `git status` in the repo → `config/*.state` and `config/selected` should
   now exist and be trackable — commit them so tuned params travel with the
   repo.
