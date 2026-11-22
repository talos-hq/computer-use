# Host tools and header metadata only. soldr provides the target sysroot.
{ pkgs ? import <nixpkgs> { } }:
let
  # PipeWire and SPA supply headers for bindgen. The driver loads PipeWire
  # at runtime, so avoid passing host shared libraries to the target linker.
  headerMetadata = pkgs.runCommand "computer-use-pipewire-header-metadata" { } ''
    mkdir -p "$out/lib/pkgconfig"
    for pc in ${pkgs.pipewire.dev}/lib/pkgconfig/*.pc; do
      sed -E '/^Libs(\.private)?:/d' "$pc" > "$out/lib/pkgconfig/$(basename "$pc")"
    done
  '';
  # soldr supplies a target-scoped search path for managed link libraries.
  # Route only PipeWire/SPA queries to our separate header-only metadata.
  pkgConfig = pkgs.writeShellScriptBin "pkg-config" ''
    for arg in "$@"; do
      case "$arg" in
        libpipewire-0.3|libspa-0.2)
          export PKG_CONFIG_PATH=${headerMetadata}/lib/pkgconfig
          export PKG_CONFIG_LIBDIR=
          unset PKG_CONFIG_SYSROOT_DIR
          break
          ;;
      esac
    done
    exec ${pkgs.pkg-config}/bin/pkg-config "$@"
  '';
in pkgs.mkShell {
  nativeBuildInputs = [ pkgs.pkg-config pkgs.binutils pkgs.patchelf
    pkgs.rustPlatform.bindgenHook ];
  buildInputs = [ headerMetadata ];
  shellHook = ''
    export PATH="${pkgConfig}/bin:$PATH"
  '';
}
