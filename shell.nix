# Dev shell for building zgicabra, in particular the statically-linked SDL2
# (sdl2-sys with the "bundled" + "static-link" features): it compiles real
# upstream SDL2 from source via cmake, which needs a pile of X11/DRM/mesa/
# alsa dev headers at build time. None of this ships in the final binary --
# it's all build-time only -- but `cargo build` needs it on PATH/CPATH to
# get there, and plain `environment.systemPackages` doesn't propagate C
# header search paths the way it propagates PKG_CONFIG_PATH (nix-shell's
# setup-hooks are what actually wire up -I/-L flags for a bare compiler
# invocation like the raw `#include <xcb/xcb.h>` SDL2's source pulls in
# regardless of whether pkg-config would've found it).
#
# Usage: `nix-shell` (or `nix-shell --run 'cargo build'`) from the repo root.
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    cmake
    pkg-config

    alsa-lib.dev

    libx11.dev
    libxext.dev
    libxrandr.dev
    libxfixes.dev
    libxrender.dev
    libxi.dev
    libxcursor.dev
    libxscrnsaver
    libxkbcommon.dev
    libxcb.dev
    libdrm.dev
    mesa
  ];

  # CMake 4 dropped support for the old cmake_minimum_required() SDL2's
  # vendored source declares; this is the documented escape hatch.
  CMAKE_POLICY_VERSION_MINIMUM = "3.5";

  # GCC 15 defaults to C23, where bool/true/false are keywords -- SDL2's
  # src/joystick/hidapi/SDL_hidapi_steam.c (pre-C99-stdbool-era code) redeclares
  # them as an enum, which only compiles under an older C standard.
  CFLAGS = "-std=gnu17";
}
