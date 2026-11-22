# Linux build environment for puppet (NixOS has no global -dev packages).
# Usage: nix-shell --run 'cd rust && soldr cargo build -p cua-driver --features portal-libei'
# Inputs mirror upstream's nix/cua-driver/package.nix.
{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    rustPlatform.bindgenHook # libclang for the libpipewire / libspa bindgen
  ];
  buildInputs = with pkgs; [
    libx11
    libxi
    libxtst
    libxext
    pipewire
    libei
    libxkbcommon
    dbus
  ];
  # The portal video route dlopens libpipewire at runtime.
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.pipewire ];
}
