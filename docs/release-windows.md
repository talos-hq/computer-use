# computer-use on Windows

Extract the ZIP and run `computer-use.exe --version` or `computer-use.exe doctor`.
`cua-driver.exe` is a byte-identical compatibility copy. To run the other tools,
use `computer-use.exe applet cua-cursor-theme` or
`computer-use.exe applet cua-perception`.

The UI Automation applet runs in the main executable. It may be unable to
interact with elevated windows because it does not have a separate UIAccess
manifest.
