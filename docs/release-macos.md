# computer-use on macOS

Extract the archive and move `ComputerUse.app` to `/Applications`. Run
`/Applications/ComputerUse.app/Contents/MacOS/computer-use doctor` to check
Accessibility and Screen Recording permissions. The app has the bundle ID
`com.taloshq.computer-use`. macOS grants permissions to this signed identity.

Computer History stores its encryption key in the login keychain. Using the
data-protection keychain in a future release would require an Apple Developer
ID provisioning profile and a migration for existing history keys.
