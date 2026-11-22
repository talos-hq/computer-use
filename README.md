# computer-use

Standalone computer-use automation driver: one executable with argv[0] multicall
applets. This is a hard fork of `trycua/cua`'s `libs/cua-driver`, MIT licensed,
at snapshot `e50137d` (v0.56.9). Internal crate names, socket paths,
`CUA_DRIVER_*` variables, and `~/.cua-driver` remain compatible. Telemetry,
the upstream updater, and `connect` remain disabled.

## Build and install

Portable Linux x86_64 releases (glibc 2.34+) include the multicall binary,
compatibility symlinks and `wayland-helper/`. See [Linux releases and Nix
installation](docs/release-linux.md) for runtime library discovery, prerequisites
and platform gaps.

```sh
./scripts/release-linux.sh
./scripts/install-local.sh --prefix "$HOME/.local" dist/computer-use-0.57.0-linux-x86_64.tar.gz
nix-build nix/package.nix
```

For a development build on NixOS, from the repository root:

```sh
nix-shell --run 'cd rust && nice -n 10 soldr cargo build -j 4 -p cua-driver --features portal-libei'
```

X11/Xi, xkbcommon and PipeWire are loaded at runtime. After rebuilding, the
uninstalled binary runs outside nix-shell without `LD_LIBRARY_PATH`; `doctor`
reports unavailable optional libraries. The portable release uses the standard
ELF loader (NixOS needs nix-ld); the Nix package patches its own loader.

The product is `rust/target/debug/computer-use` (use `--release` for a release
build). Put that executable in your installation directory, then run:

```sh
computer-use install-links                 # next to the resolved executable
computer-use install-links --dir ~/.local/bin
computer-use applet --list
```

Use the portable installer above or this build plus `install-links` for this
fork. Other retained upstream release/source installer scripts and
language-package builders still target upstream artifact names and releases.
The legacy developer flags of `scripts/install-local.sh` retain their old
behavior; its default and `--prefix` mode install the built portable release.

The link helper creates absolute symlinks to the running executable, is
idempotent, and refuses to overwrite other files or links. On Windows it creates
`.exe` symlinks and requires Developer Mode or permission to create symlinks.
If linking is unavailable, invoke an applet explicitly:

```sh
computer-use applet cua-cursor-theme --help
computer-use applet cua-perception --manifest /path/model-manifest.json \
  --onnx-runtime-library /path/libonnxruntime.so \
  --extension-id cua-perception --extension-version 0.56.9
```

| Invocation basename (also accepts `.exe`) | Behavior |
| --- | --- |
| `computer-use`, `cua-driver`, or an unknown basename | Driver CLI / MCP server |
| `cua-cursor-theme`, `computer-use-cursor-theme` | Cursor-theme authoring CLI |
| `cua-perception`, `computer-use-perception` | Perception worker |
| `cua-driver-uia` (Windows only) | UIA worker, requiring `--authorized-parent-pid` |

`computer-use applet <name> [args...]` accepts the auxiliary basenames above,
plus `cursor-theme` and `perception` as short names. `install-links` installs all
listed aliases. Existing Hermes commands such as `cua-driver mcp` keep using the
driver CLI. The `cua-driver` link retains the legacy help/version spelling;
`computer-use --version` prints the new product name.

The KWin C++ plugin in `kwin-target-helper/`, GNOME Shell extension in
`wayland-helper/`, `libcua_driver_sdk`, and UniFFI/node bindings are unchanged.
They are plugins/libraries, not separate applet executables. Contract/bindgen
programs remain developer tools, outside the shipped executable.

The Windows UIA code and its authorization checks are linked only on Windows.
A single executable has one PE manifest: `computer-use` retains the driver's
`uiAccess=false` manifest so stdio MCP and normal process launches work. The
UIA applet therefore does **not** retain the old separate helper's UIAccess
integrity / UIPI bypass. It has no public forwarding route today; enabling
UIAccess elevation is outside this change. Windows and macOS need native
validation; this change is validated on Linux.

## Local visual perception assets

The driver launches `current_exe()` with `applet cua-perception`; it never
selects an installed extension's worker or automatically downloads code/models.
The existing signed/local extension administration code is retained for a
reviewable diff, but its active pointer, catalog, and Ed25519 trust root are
not used by the built-in perception resolver. Installing an old extension does
not configure the built-in worker.

Real inference requires local assets that are **not included in this repository
or the executable**. Defaults are under `~/.cua-driver/perception/` (or
`$CUA_DRIVER_RS_HOME/perception/`):

- `model-manifest.json`, a completed manifest based on
  `rust/crates/cua-perception/models/model-manifest.template.json`; its target,
  runtime version and library SHA-256 must match the actual platform library.
- `artifacts/omniparser-icon-detect-1280-opset17.onnx` (OmniParser v2 detector).
- `artifacts/ppocrv5-mobile-det.onnx` (PPOCRv5 detection model).
- `artifacts/ppocrv5-mobile-rec-en.onnx` (English recognition model).
- `artifacts/ppocrv5-mobile-rec-en-inference.yml` (recognition dictionary).
- `runtime/libonnxruntime.so`, `runtime/libonnxruntime.dylib`, or
  `runtime/onnxruntime.dll`, plus that runtime's dependent shared libraries.
  The template pins ONNX Runtime **1.26.0**; `ort` uses `load-dynamic` and the
  worker checks the runtime version and hashes before inference.

For real inference, the installed executable's native dependencies must also
resolve without environment overrides: the contained worker clears inherited
environment variables, including `LD_LIBRARY_PATH`. A raw Nix debug build may
need loader-path packaging before it can run inference in that sandbox.

Artifact paths are relative to the manifest, with hashes verified by the worker.
`CUA_PERCEPTION_MANIFEST` overrides the manifest path; the existing
`CUA_PERCEPTION_RUNTIME` overrides the runtime library path. Set these before
starting the driver. Missing assets produce an error; the driver never
substitutes fixture detections. `applet cua-perception --fixture` is an explicit
deterministic protocol-test backend only. Health/self-test gate flags locate
assets beside the executable (or via `CUA_PERCEPTION_RUNTIME`); use the explicit
`--manifest ... --onnx-runtime-library ...` form for externally stored assets.

See the checked-in [model ledger](rust/crates/cua-perception/models/MODEL_LEDGER.md),
[licenses and notices](rust/crates/cua-perception/models/THIRD_PARTY_NOTICES.md),
and [source offer](rust/crates/cua-perception/models/SOURCE_OFFER.md). No model
acquisition, export, or inference-quality validation is claimed by this PR.

## Integration surfaces

- **Cua as an agent MCP or CLI:** MCP-capable agents connect directly to
  `cua-driver mcp`; shell-oriented agents and automation can use
  `cua-driver call`. No generated Cua language client is required.
- **Cua as an application SDK:** Python applications import `cua_driver`;
  TypeScript applications import `@trycua/cua-driver`. Both package roots call
  the same in-process native runtime through generated UniFFI bindings. The
  safe Rust SDK and generated bindings sit above a versioned C ABI; they do not
  require a daemon for direct application use.

The language packages are for client applications, not agents. They contain no
language-native MCP facade and have no `/sdk`, `/mcp`, or `/native` public
suffix. MCP remains implemented by the `cua-driver` executable as the
runtime-neutral agent boundary.

## Computer History macOS preview

> **Deprecated.** Computer History is deprecated. It is off by default, agents are no longer told to call `history_status` or `history_query`, and both tools will be removed in the release after 26 Nov 2026. To remove stored data, run `cua-driver history delete --yes`.

Nightly macOS builds can provide an opt-in, encrypted history of actions
performed through Cua Driver. The preview stores a strict metadata allowlist,
stays local, and exposes permission-gated `history_status` and `history_query`
tools for read-only agent hydration. It never stores screenshots, typed text,
clipboard contents, raw arguments or results, accessibility trees, paths,
window titles, or URLs.

See [Try the Computer History macOS
preview](docs/computer-history-preview.md) for installation, lifecycle,
inspection, deletion, and stable-channel return instructions. The [architecture
and staged plan](docs/computer-history-architecture.md) defines the format,
security boundary, release gates, and later NVIDIA OpenShell integration.

## Permission modes

`unrestricted` is the default with no flags or environment variables. Driver
approval prompts are disabled for desktop input, screenshots/recording, clipboard,
files, process termination, history, and existing browser profiles. No
`--dangerously-bypass-approvals` acknowledgement is required; that flag remains a
compatibility alias. Automation has the permissions of the account running it.

Opt into upstream restrictions with `computer-use serve --permission-mode standard`
or `CUA_DRIVER_PERMISSION_MODE=standard`. Existing-profile attachment in standard
mode requires `--grant existing-profile` at daemon startup or an embedding
`DriverAuthorizationHost`. `bounded` requires a reviewed capability manifest and
`--approve-capability-manifest`. Explicit policies and manifests still narrow
access in every mode. `CUA_DRIVER_DISABLE_UNRESTRICTED=1` forces the default to
standard; explicitly requesting unrestricted then fails with `UnrestrictedDisabled`.
`doctor`, `status`, and help report the effective mode and unrestricted default.
Restart the daemon to change its mode.

On Linux Wayland, ordinary builds include portal input and capture, and native
Wayland routing is enabled by default even when XWayland exports `DISPLAY`.
`CUA_DRIVER_RS_ENABLE_WAYLAND=0` retains the compatibility XWayland backend.
The daemon
lazily creates one RemoteDesktop session, selects keyboard and pointer devices,
attaches ScreenCast monitor sources to that same session, and calls Start once.
Input uses ConnectToEIS; screenshots grab a PipeWire frame and recordings read the
same monitor node. Stopping a recording leaves the daemon's session alive. A
single restore token in `$XDG_CONFIG_HOME/cua-driver/desktop-session.token` (normally
`~/.config/cua-driver/desktop-session.token`) requests persistence until explicitly
revoked. The old input/video tokens are ignored, so upgrading can require one new
dialog. Persistence depends on the desktop portal honouring restore tokens; a
revoked or expired grant needs approval again.

A denied or unanswered dialog times out after 60 seconds and is remembered until
the daemon restarts, so repeated calls do not open additional dialogs. Native
compositor capture can avoid portals entirely. When the combined portal cannot
be created, the legacy Screenshot portal remains a compatibility fallback; some
desktops, including KDE, can prompt for each fallback screenshot. A failed frame
from an established session never falls back to another consent dialog. The
shared capture currently supports a monitor covering the whole desktop action
frame; it refuses a partial multi-monitor selection instead of stretching its
image into incorrect desktop coordinates. Portal stream composition remains
unsupported.

macOS Accessibility and Screen Recording are separate TCC grants and cannot be
merged into one OS prompt. Windows UAC/secure-desktop grants and browser-owned
remote-debugging or keychain dialogs also remain controlled by those applications
and the OS. The unrestricted default removes the driver's authorization prompts
on all platforms; it cannot remove independent OS/application permission dialogs.

## Repository Layout

| Path                            | Purpose                                                                |
| ------------------------------- | ---------------------------------------------------------------------- |
| `rust/`                         | Cargo workspace for the daemon, UniFFI SDK, platform crates, and tests |
| `python/`                       | Python SDK, bundled-binary wrapper, and package tests                  |
| `contract/`                     | Experimental generated SDK contract and fixtures                       |
| `typescript/`                   | Generated TypeScript SDK                                               |
| `tests/fixtures/`               | Source-built GUI harness apps and shared fixtures                      |
| `rust/crates/cua-driver/tests/` | Hermetic Rust integration tests for the driver                         |
| `rust/crates/cua-driver-e2e/tests/` | Desktop E2E suites for the GUI harnesses                       |
| `scripts/`                      | Install, uninstall, local build, and VM sync helpers                   |
| `docs/`                         | Small repo-local specs that are not part of the hosted docs site       |

Start with `rust/README.md`, `rust/crates/cua-driver-e2e/tests/README.md`, and
`tests/fixtures/README.md` when changing driver behavior or tests.

The canonical GUI runners execute the complete Rust harness catalog:

```text
Linux X11/session: scripts/ci/linux/run-rust-e2e.sh
Linux Sway:        scripts/ci/linux/run-rust-e2e-wayland.sh
Linux niri:        CUA_E2E_WAYLAND_SESSION=niri scripts/ci/linux/run-rust-e2e-wayland.sh
Linux nested:      scripts/ci/linux/run-rust-e2e-inject.sh
Linux GNOME/KDE:   scripts/ci/linux/run-rust-e2e-desktop.sh <gnome|kde>
Linux real Xorg:   scripts/ci/linux/run-rust-e2e-desktop.sh xorg
Windows:           .\scripts\ci\windows\run-rust-e2e.ps1 -RequireGui
macOS:             scripts/ci/macos/run-rust-e2e.sh
```

The Sway runner creates a controlled stock-wlroots session; Nix owns the Linux
source/package gate and the optional nested `cua-compositor` session. GNOME,
KDE, real Xorg, Windows, and macOS use an existing graphical login (Windows:
an interactive console or RDP session; macOS: a logged-in session with
Accessibility and Screen Recording). See
[`docs/test-harnesses-guide.md`](docs/test-harnesses-guide.md) and, for macOS in
Lume, [`tests/runners/macos-lume/README.md`](tests/runners/macos-lume/README.md).
Keep build artifacts, VM logs, and local verification journals out of git
unless they become a stable fixture or contributor doc.

The contract-first SDK architecture is documented in
[`contract/README.md`](contract/README.md). The Rust contract crate generates
the checked-in manifest and supplies the typed inputs used by the live daemon
and UniFFI SDK.

The imported SDK bindings are generated by
`scripts/generate-uniffi-bindings.mjs`. `CuaDriver.create()` loads the native
runtime in the application process; the daemon-compatible `connect()` path is
retained for external clients. The stable native boundary is declared in
[`rust/include/cua_driver_abi.h`](rust/include/cua_driver_abi.h). That header is
generated from the Rust `#[repr(C)]` exports with
`cargo run -p cua-driver-bindgen --bin cua-driver-abi-header`; CI runs the same
command with `--check` so the implementation and distributed header cannot
drift. UniFFI separately generates its private FFI scaffolding and the Python
and TypeScript SDK bindings; that implementation-specific scaffolding is not
the stable public `cua_driver_*_v1` C contract.
Python wheels and the root/native npm packages are assembled from the same
`cua-driver-rs-v*` release artifacts and publish with the exact Rust release
version; `.github/scripts/validate_release_versions.py` rejects source drift.

For direct agent integrations, see the
[`examples/agent-sdks`](examples/agent-sdks/README.md) Codex and Claude Agent
SDK examples. They connect the agent to `cua-driver mcp` without importing a
generated Cua client.

## Muse Code

[Muse Code](https://research.meta.ai/blog/introducing-muse-code-and-muse-spark-1-2)
can use Cua Driver as a local stdio MCP server on macOS and Linux. Merge the
following entry into `$XDG_CONFIG_HOME/muse/settings.json`, or
`~/.config/muse/settings.json` when `XDG_CONFIG_HOME` is not set:

```json
{
  "mcp_servers": {
    "cua-driver": {
      "enabled": true,
      "transport": "stdio",
      "command": "/absolute/path/to/cua-driver",
      "args": ["mcp"]
    }
  }
}
```

Use the installed binary's absolute path, which `command -v cua-driver` prints.
Merge the `cua-driver` entry with any existing `mcp_servers` instead of
replacing the settings file. Start a new Muse session after saving the file;
Muse loads MCP servers at session startup.

Contributor documentation:

- `docs/cursor-themes.md` documents the default semantic cursor and custom
  dotLottie authoring contract.
- `docs/test-matrix.md` maps unit and canonical harness E2E suites.
- `docs/action-support.md` is the empirical platform behavior ledger.
- `docs/test-harnesses-guide.md` explains fixture and runner ownership.
- `docs/linux-desktop-validation.md` covers representative Linux sessions.
- `docs/linux-support-completion-plan.md` preserves the historical Linux plan.

## Claude Code registration

Register the standard MCP server:

```bash
claude mcp add --transport stdio cua-driver -- cua-driver mcp
```

The old `--claude-code-computer-use-compat` flag has been removed, and
`cua-driver` now exits with an error that names it. It had no effect on the
tool surface, so delete it from older registrations and use the command above.
There is no standalone `screenshot` tool. Use `get_window_state` for a window
tree plus screenshot, `get_desktop_state` for a full-display screenshot, and
`zoom` for a cropped region.

## macOS process identity and permissions

macOS attributes Accessibility and Screen Recording grants to a responsible app identity, not simply to an executable path. Use one of these supported launch modes:

- **Standalone:** install `CuaDriver.app`, grant permissions to it, and start its daemon with `cua-driver permissions grant`. The installed `cua-driver mcp` CLI may proxy through this daemon automatically.
- **Explicit direct MCP:** `cua-driver mcp --direct` makes the MCP process own
  its runtime. On macOS this deliberately uses the spawning host's TCC
  attribution and does not provide the AppKit cursor overlay without a
  certified host adapter.
- **Embedded:** have the macOS app that owns the grants use the generated `EmbeddedCuaDriverHost` to spawn a private daemon and return both SDK and MCP connection details. The daemon stays in the app's responsibility chain and inherits its grants. A gateway, terminal, or unrelated helper must not spawn the daemon on the app's behalf. `@trycua/cua-driver/embedded` is an organizational alias for the same Rust host exported at the package root; it has no separate lifecycle implementation.

Directly spawning a raw `cua-driver serve` outside `CuaDriver.app` without embedded mode is unsupported: it has no stable bundle identity for TCC attribution. Do not grant permissions to arbitrary binary paths or rely on that configuration in production. See [`rust/Skills/cua-driver/EMBEDDING.md`](rust/Skills/cua-driver/EMBEDDING.md) for the embedding contract and examples.

## Publishing the agent skill to ClawHub

The canonical skill source is `rust/Skills/cua-driver`. It is published as one
cross-platform ClawHub skill at `@cua/driver`; the bundle includes the
macOS, Windows, and Linux documents. Direct installs through `cua-driver skills
install` still keep only the host OS document unless `--all-platforms` is used.

ClawHub releases have their own explicit license boundary. The repository stays
under MIT, while every skill copy published through ClawHub is distributed
under MIT-0. Before a release, retain an internal record that Cua AI has the
right to distribute every bundled file under MIT-0.

Pull requests that change the skill run a publish dry-run with the pinned
ClawHub CLI. A real release is available only through the
`ClawHub: cua-driver skill` workflow's manual dispatch. The publish job requires
all of the following:

1. Dispatch the workflow from `main`.
2. Enter a version that matches both `rust/Cargo.toml` and the `metadata.version` field
   in `rust/Skills/cua-driver/SKILL.md`.
3. Confirm the MIT-0 rights check in the workflow form.
4. Configure a repository Actions secret named `CLAWHUB_TOKEN` for a publisher
   that can release under the selected owner. The default owner is `cua`.

The workflow pins the ClawHub CLI, records the source repository, commit, ref,
and path, and uploads the JSON publish result as an Actions artifact.

After publishing, inspect and scan the exact version, then install it into an
empty work directory:

```bash
npx --yes clawhub@0.23.1 inspect @cua/driver --version 0.8.3 --files
npx --yes clawhub@0.23.1 scan --slug driver --version 0.8.3 --update
npx --yes clawhub@0.23.1 --workdir /tmp/cua-driver-clawhub-smoke \
  install @cua/driver
```

Confirm that `MACOS.md`, `WINDOWS.md`, and `LINUX.md` are present, run
`cua-driver doctor`, and perform a read-only `list_apps` call through OpenClaw.
If a release is faulty, publish the last known-good content as a new patch
version. Do not delete the current latest version before a replacement exists.
