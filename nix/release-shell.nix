# glibc 2.34 / GCC 11 baseline (NixOS 22.05); full portal features still use
# current PipeWire headers. Compilation always goes through soldr.
{ headerPkgs ? import <nixpkgs> { } }:
let
  baseline = import (builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/ce6aa13369b667ac2542593170993504932eb836.tar.gz";
    sha256 = "0d643wp3l77hv2pmg2fi7vyxn4rwy0iyr8djcw1h5x72315ck9ik";
  }) { system = "x86_64-linux"; };
  # pipewire-sys/libspa-sys need headers, but their usual .pc files also emit
  # -lpipewire. Our entry points are dlopened, so remove those native link flags
  # instead of handing an unneeded, newer-glibc shared object to the linker.
  headerMetadata = baseline.runCommand "computer-use-pipewire-header-metadata" { } ''
    mkdir -p "$out/lib/pkgconfig"
    for pc in ${headerPkgs.pipewire.dev}/lib/pkgconfig/*.pc; do
      sed -E '/^Libs(\.private)?:/d' "$pc" > "$out/lib/pkgconfig/$(basename "$pc")"
    done
  '';
  compilerRuntime = baseline.lib.makeLibraryPath [ baseline.stdenv.cc.cc.lib ];
in baseline.mkShell {
  nativeBuildInputs = [ baseline.pkg-config baseline.binutils baseline.patchelf
    baseline.rustPlatform.bindgenHook ];
  buildInputs = [ headerMetadata ];
  PKG_CONFIG_PATH = "${headerMetadata}/lib/pkgconfig";
  # Override any user Cargo linker selection as well as CC: the shipped ELF
  # must link against the baseline glibc, including SDK dependency outputs.
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "${baseline.stdenv.cc}/bin/cc";
  # Generic soldr/toolchain executables enter through nix-ld. Keep its newer
  # libgcc/libstdc++ from leaking into glibc-2.34 build scripts and libclang.
  LD_LIBRARY_PATH = compilerRuntime;
  NIX_LD_LIBRARY_PATH = compilerRuntime;
}
