# computer-use Linux release

This is the talos-hq MIT fork of Cua's driver. Telemetry and updates are disabled;
input is unrestricted by default. KDE target guards and the shared persistent
RemoteDesktop + ScreenCast portal session are included.

The x86_64 and aarch64 tarballs require glibc 2.17 or later. Desktop features also need the runtime libraries and display services described below.
Extract its single directory and run `./computer-use --version` or
`./computer-use doctor`. The `cua-driver`, `cua-cursor-theme` and `cua-perception`
names are relative symlinks to the same multicall executable. Keep
`wayland-helper/` alongside the binary; these are upstream's optional GNOME
extension sources and installer, not a replacement for the KDE native path.
The archive preserves the upstream MIT notice in `LICENSE`.

X11, Xi, xkbcommon and PipeWire are optional runtime libraries. Missing X11 input
libraries do not prevent startup, Wayland window enumeration, portal pointer
input or capture. Keyboard layout handling needs xkbcommon; typing needs version
1.0 or later. `doctor` reports missing X11/Xi/xkbcommon libraries. Desktop library
search uses the normal loader, absolute directories in `COMPUTER_USE_LIBRARY_PATH`,
`LD_LIBRARY_PATH` and `NIX_LD_LIBRARY_PATH`, standard multiarch/lib directories,
`/run/current-system/sw/lib`, `/run/current-system/sw/share/nix-ld/lib`, then library paths mapped by the current user's
running compositor/desktop processes. `COMPUTER_USE_LIBRARY_PATH` is the explicit
fallback for unusual installations; it is a colon-separated list of absolute
library directories. Do not point it at untrusted directories.

On NixOS the direct tarball binary needs `programs.nix-ld.enable = true` for the
standard `/lib64/ld-linux-x86-64.so.2` interpreter. No nix-shell or LD_LIBRARY_PATH
is required. Prefer the Nix package below to install without nix-ld: it patches
the loader and supplies runtime library paths, including PipeWire's SPA plugins.
A glibc binary cannot start on arbitrary NixOS machines without either an ELF
loader at the standard path or Nix installation patching; desktop dlopen alone
cannot solve that loader requirement.

## Build and install

From a clean checkout on Linux x86_64:

```sh
./scripts/release-linux.sh
COMPUTER_USE_RELEASE_ARCH=aarch64 ./scripts/release-linux.sh
./scripts/install-local.sh --prefix "$HOME/.local" dist/computer-use-0.57.2-linux-x86_64.tar.gz
```

The release script requires Nix, soldr, safe-mktemp and safe-rm on PATH. It uses
`nix/release-shell.nix` for host build tools and current PipeWire headers.
Header-only pkg-config metadata keeps host PipeWire libraries out of the target
link. The script runs `soldr build --target "$arch-unknown-linux-gnu.2.17"`
with the lockfile so soldr selects the glibc 2.17 sysroot. Both architectures
build on x86_64 because soldr ships an x86_64-hosted compiler for these targets. It strips the
binary, sets the standard glibc loader, removes build-machine RPATHs, rejects
hard-linked desktop libraries, preserves symlinks and writes SHA-256. Rust's
release profile also strips symbols and uses fat LTO. CI runs each packaged binary on a matching glibc 2.17 userspace. Build archives stay out
of git. Normal development still uses `nix-shell` and soldr.

`install-local.sh` requires Python 3. It also accepts an unpacked release directory or a built binary;
without a source it selects the release tree. It installs the executable and
helper directory under `<prefix>/lib/computer-use/` and relative command symlinks
under `<prefix>/bin/`. Add that bin directory to PATH. It does not start services
or configure any agent.

```sh
nix-build nix/package.nix
./result/bin/computer-use doctor
nix-env -i -f nix/package.nix
```

The Nix derivation packages the checksum-pinned prebuilt release and uses
`autoPatchelfHook` plus runtime wrappers. This avoids a Nix Rust builder invoking
bare cargo: all compilation remains in the soldr release script. For a local
archive before publication, pass `--arg src /absolute/path/to/release.tar.gz`.

The Nix package targets Linux x86_64. The portable archives support native
Linux builds on x86_64 and aarch64.
