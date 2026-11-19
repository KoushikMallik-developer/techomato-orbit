# Releasing Techomato Orbit

## What a release produces

`npm run release` (which runs `tauri build`) writes to `src-tauri/target/release/`:

- `techomato-orbit.exe`, the app itself.
- `bundle/nsis/Techomato Orbit_<version>_x64-setup.exe`, the setup program. It asks whether to
  install for the current user or for all users (per-machine, needs administrator).
- `bundle/msi/Techomato Orbit_<version>_x64_en-US.msi`, a machine-wide installer for IT use.
- A `.sig` file next to each installer. These are the update signatures.

Windows service control and ending protected processes ask for administrator permission (a UAC
prompt) in both install types. Known limit: a per-user install lives in a folder that any program
of that user can overwrite, so such a program could take over the prompt. The all-users install
(Program Files) does not have this weakness.

The setup includes the WebView2 bootstrapper, so Windows 10 machines without WebView2 get it
installed automatically.

## One-time setup (the signing key)

Updates are signed. Orbit refuses an update whose signature does not match the public key stored
in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).

1. The key pair was generated with `npx tauri signer generate -p <password>`. The private key is
   encrypted with a password. Both live in `~/.tauri/` (`techomato-orbit.key` and
   `techomato-orbit.key.password`). **Move the password into a password manager and delete the
   `.password` file**, and keep a backup of the key file. The private key must never be committed.
   If the key or password is lost, installed copies of Orbit can never be updated again and users
   must reinstall.
2. In the GitHub repository, add these Actions secrets:
   - `TAURI_SIGNING_PRIVATE_KEY`: the contents of the private key file.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password.
3. The repository must be public. The updater reads `latest.json` from the release URL without signing
   in, so with a private repo every update check fails with "Could not check for updates".
4. To use a different key, run `npx tauri signer generate -w <path>` and put the new public key in
   `tauri.conf.json`. Releases signed with the old key will not be accepted by builds with the new one. Re-sign
   `src-tauri/tests/fixtures/updater/sample.bin` with the new key (`npx tauri signer sign`) so the
   signature test keeps passing.

## Cutting a release

1. Change the version in all three places (they must match): `package.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`.
2. Merge to `master`, then tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The **Release** workflow checks the tag against the three versions, builds on `windows-latest`,
   signs the installers, and creates a **draft** GitHub release with the installers and `latest.json`.
4. Open the draft, check the files, and publish it. Only a published release is visible to the
   updater (`.../releases/latest/download/latest.json`).

## How updating works

Settings has an Updates section. By default Orbit checks only when the user presses the button; an
optional checkbox checks once at launch (showing a notification, never installing). Pressing
Install asks for confirmation, downloads and verifies the signature, stops services the same way
Quit does, then runs the installer. A per-machine install shows a Windows administrator prompt.

To test against a local manifest in a development build, set `ORBIT_UPDATE_URL` to its address
before `npm start`. This override does not exist in release builds.

## Code signing (not set up)

The installers are signed for updates but not code-signed with a Windows certificate, so SmartScreen
will warn "unknown publisher" until one is bought and configured. That is a separate step.
