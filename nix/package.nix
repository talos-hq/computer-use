# Prebuilt soldr release: Nix must not invoke an unmanaged Rust compiler.
{ pkgs ? import <nixpkgs> { }, src ? null }:
let
  version = "0.57.2";
  runtime = with pkgs; [ libx11 libxi libxtst libxext libxkbcommon pipewire ];
in pkgs.stdenvNoCC.mkDerivation {
  pname = "computer-use";
  inherit version;
  src = if src != null then src else pkgs.fetchurl {
    url = "https://github.com/talos-hq/computer-use/releases/download/computer-use-v${version}/computer-use-${version}-linux-x86_64.tar.gz";
    sha256 = "529857b9ebcb1a22e4b7450f2f7e935e96374df2c60385a9d84115e5e15002e8";
  };
  nativeBuildInputs = [ pkgs.autoPatchelfHook pkgs.makeWrapper ];
  buildInputs = [ pkgs.stdenv.cc.cc.lib ];
  dontConfigure = true;
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/libexec/computer-use" "$out/bin" "$out/share/licenses/computer-use"
    cp computer-use "$out/libexec/computer-use/"
    cp -R wayland-helper "$out/libexec/computer-use/"
    cp LICENSE "$out/share/licenses/computer-use/"
    makeWrapper "$out/libexec/computer-use/computer-use" "$out/bin/computer-use" \
      --prefix COMPUTER_USE_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtime}" \
      --prefix SPA_PLUGIN_DIR : "${pkgs.pipewire}/lib/spa-0.2"
    for applet in cua-driver cua-cursor-theme cua-perception; do
      # A separate wrapper preserves argv[0] multicall dispatch.
      ln -s computer-use "$out/libexec/computer-use/$applet"
      makeWrapper "$out/libexec/computer-use/$applet" "$out/bin/$applet" \
        --prefix COMPUTER_USE_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtime}" \
        --prefix SPA_PLUGIN_DIR : "${pkgs.pipewire}/lib/spa-0.2"
    done
    runHook postInstall
  '';
  meta = {
    description = "Portable computer automation driver (talos-hq fork of Cua)";
    homepage = "https://github.com/talos-hq/computer-use";
    license = pkgs.lib.licenses.mit;
    platforms = [ "x86_64-linux" ];
    mainProgram = "computer-use";
  };
}
