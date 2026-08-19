# zgicabra performance box: every zgicabra-specific system concern in one
# place, so a replacement machine can be brought up from this repo alone.
{ config, lib, pkgs, ... }:

{
  #
  # Boot: performance mode is the base system -- always boots by default,
  # no keyboard needed, no login anywhere in the chain. Dev mode is a
  # specialisation, reachable only by selecting it at the GRUB menu within
  # the timeout. See sys/system-config.plan.md for why this mechanism was
  # chosen over hardware auto-detection.
  #
  boot.loader.timeout = 3;
  boot.loader.grub.useOSProber = false;

  boot.kernelParams = [
    "consoleblank=0"         # never blank the KMSCON TUI mid-set
    "usbcore.autosuspend=-1" # keep Hydra/MIDI USB devices from idling out
  ];

  #
  # Audio (MusNix + kernel modules) -- shared by both modes. Pipewire stays
  # configured at the top level of configuration.nix; zgicabra only ever
  # talks to it via cpal's ALSA path regardless of which mode is active.
  #
  musnix.enable = true;
  boot.kernelModules = [ "snd-virmidi" ];

  # Required for kmscon to render the TUI correctly -- must be specified as
  # exactly "Terminus (TTF)" in the service below (see todo.md).
  fonts.packages = with pkgs; [ terminus_font_ttf ];

  #
  # Hydra + panel udev permissions. Supersedes sys/udev-rules (folded in
  # here) and configuration.nix's old inline extraRules block -- applied
  # automatically on every rebuild, no manual `cp` step needed on a fresh
  # machine.
  #
  services.udev.extraRules = ''
    # Razer Hydra plus all devices on same bus (for some reason)
    SUBSYSTEM=="usb", MODE="0666", GROUP="plugdev"
    SUBSYSTEM=="usb", GROUP="adm", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="0300", MODE="0666"
    SUBSYSTEM=="usb_device", GROUP="adm", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="0300", MODE="0666"

    # TURZX 5" USB panel (QinHeng UsbMonitor, CDC-ACM) - sleeping identity
    SUBSYSTEM=="tty", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="ca21", GROUP="plugdev", MODE="0660"
    # TURZX 5" USB panel - awake identity (varies by firmware/board rev, cover all known ones)
    SUBSYSTEM=="tty", ATTRS{idVendor}=="0525", ATTRS{idProduct}=="a4a7", GROUP="plugdev", MODE="0660"
    SUBSYSTEM=="tty", ATTRS{idVendor}=="1d6b", ATTRS{idProduct}=="0121", GROUP="plugdev", MODE="0660"
    SUBSYSTEM=="tty", ATTRS{idVendor}=="1d6b", ATTRS{idProduct}=="0106", GROUP="plugdev", MODE="0660"
  '';

  #
  # Performance-mode service: KMSCON exec's zgicabra directly on vt1, no
  # login anywhere in the chain. Base config only -- disabled again inside
  # the dev specialisation below, where vt1 belongs to the X session
  # instead.
  #
  # Runs as root (unset User), matching the conventional getty-replacement
  # pattern -- VT-ownership ioctls (KDSETMODE, VT_ACTIVATE) traditionally
  # need root or CAP_SYS_TTY_CONFIG, and the hand-tested ~/kms prototype
  # this is based on needed sudo for the same reason. This is a
  # serviceConfig field, not a login/PAM path -- doesn't reintroduce a
  # login prompt.
  #
  # --login is required for kmscon to actually run the `-- ARGV` we give it
  # at all -- without it, kmscon silently ignores everything after `--` and
  # always execs its own hardcoded default (`/bin/login -p`, which doesn't
  # exist at that literal path on NixOS, hence "failed to exec child
  # /bin/login"). See `kmscon --help`'s `-l, --login` entry.
  #
  # ARGV is bin/zgicabra-launch, not the zgicabra binary directly -- --login
  # also wipes the exec'd child's environment entirely (even PATH), so the
  # real binary's PATH/XDG_RUNTIME_DIR/PIPEWIRE_RUNTIME_DIR have to be
  # rebuilt right before it runs rather than set here via Environment=. See
  # zgicabra-launch's own comment and AGENTS.md's "Performance-mode launch"
  # note.
  #
  # zgicabra never logs in on this box (no keyboard, no getty), so without
  # lingering there's no user@1000 session and no pipewire daemon for the
  # ALSA backend to reach -- hence `users.users.zgicabra.linger` below.
  #
  systemd.services."getty@tty1".enable = false;

  users.users.zgicabra.linger = true;

  systemd.services.zgicabra = {
    description = "zgicabra performance instrument (KMSCON TUI)";
    wantedBy = [ "multi-user.target" ];
    after = [ "local-fs.target" "systemd-udev-settle.service" "user@1000.service" ];
    conflicts = [ "getty@tty1.service" ];
    serviceConfig = {
      ExecStart = ''
        ${pkgs.kmscon}/bin/kmscon --vt=1 --no-switchvt --login \
          --font-engine=pango --font-name="Terminus (TTF)" --font-size=20 \
          -- /home/zgicabra/zgicabra/bin/zgicabra-launch
      '';
      WorkingDirectory = "/home/zgicabra/zgicabra/target/release";
      Restart = "on-failure";
      RestartSec = 1;
      StartLimitIntervalSec = 60;
      StartLimitBurst = 5;
    };
  };

  #
  # Dev specialisation: full Xfce/lightdm desktop. Reachable only
  # by selecting it at the GRUB menu.
  #
  specialisation.dev.configuration = {
    services.xserver.enable = true;
    services.xserver.displayManager.lightdm.enable = true;
    services.xserver.desktopManager.xfce.enable = true;
    services.xserver.xkb = {
      layout = "nz";
      variant = "";
    };

    services.displayManager.autoLogin.enable = true;
    services.displayManager.autoLogin.user = "zgicabra";

    # The real boot service must not fight the X session for vt1 here.
    systemd.services.zgicabra.enable = lib.mkForce false;
    systemd.services."getty@tty1".enable = lib.mkForce true;
  };
}
