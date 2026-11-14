# Updates and releases

[English](releases.md) · [简体中文](releases.zh-CN.md)

Official repository: [dBevil-z/FileManager](https://github.com/dBevil-z/FileManager). Download packages and notes from [GitHub Releases](https://github.com/dBevil-z/FileManager/releases/latest).

## Updating in the app

Open **Settings → About and updates → Software updates**, or the update panel on **About FileManager**, and choose **Check for updates**. Checks are on demand; no background check, automatic download or automatic restart is enabled.

A newer stable release shows its notes and a **Download and install** button. Downloads use HTTPS. The embedded public key verifies both the artifact signature and its signed version before installation. Invalid signatures, failed downloads and active background jobs prevent installation. Choose **Restart now** when ready; the app waits for browsing-session persistence and checks background jobs again.

The editable GitHub display URL does not change the trusted update source or key. The fixed feed is:

```text
https://github.com/dBevil-z/FileManager/releases/latest/download/latest.json
```

| Installation                | Update behavior                                                                                                                                                                                                       |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| User-install script         | Download the signed `.bin`, verify it, write a temporary file in the owned installation directory and replace atomically. Keep `filemanager.previous` for manual rollback. No sudo.                                   |
| AppImage                    | Tauri verifies and replaces the current AppImage. Its directory must be writable.                                                                                                                                     |
| deb                         | Download the signed deb, then let `pkexec /usr/bin/dpkg -i` request system authorization. The app never collects a password. Cancellation or missing pkexec produces an error; manual installation remains available. |
| Source build / unknown path | Check versions, then install manually from Releases or rebuild.                                                                                                                                                       |

RPM is not published. Its update adapter is reserved for future packaging; RPM compatibility is not claimed.

Version 0.2.3 and earlier only opened a web page. Install 0.2.4 or later once to enable the updater. Use one installation method to avoid duplicate launchers or an older tray process. Save edits and finish transfers before upgrading. Workspaces, settings, sessions and operation history are retained.

The user-install directory is `$XDG_DATA_HOME/filemanager-desktop/bin`, or `~/.local/share/filemanager-desktop/bin` by default. To roll back, fully exit the app, copy `filemanager.previous` to `filemanager`, then launch again. For deb/AppImage, install the matching older Release. With only the current version published, a successful check correctly reports **You are up to date**.

## Maintainer publication

1. Update the versions in `package.json`, `src-tauri/tauri.conf.json`, both crate manifests, lockfiles and desktop metadata. Add bilingual `docs/releases/vVERSION.md` notes.
2. Run frontend, Rust/native and release-script tests plus formatting. Run `python3 scripts/release.py check-version --tag vVERSION`.
3. Push source, create an annotated stable tag, then push it; for example `git tag -a v0.2.5 -m 'FileManager 0.2.5'` and `git push origin v0.2.5`.
4. The [release workflow](../.github/workflows/release.yml) runs checks, builds deb/AppImage/user assets, then generates version-bound signatures, `latest.json` and `SHA256SUMS`. It publishes only after all assets are available.
5. Download and validate the published packages and native startup. Explicitly run the Rust `published_release_can_upgrade_an_isolated_user_install` test to exercise the real GitHub feed and an isolated upgrade.

Manual workflow dispatch accepts an existing stable tag. Public assets are immutable: publish a new version for corrections. Regular source pushes and PRs run checks without publishing. Repository builds use official dependency sources, not a maintainer's private mirror.

## Signing-key custody

Actions uses two encrypted repository secrets: `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key is in Tauri configuration. Never put private signing material in Git, screenshots, logs or Release assets.

The initial local key is outside the repository at `~/.local/share/filemanager-signing/`, with a 0700 directory and 0600 private-key/password files. The maintainer should keep an offline backup. GitHub cannot return configured Actions secrets. **Losing the key prevents installed clients from trusting future updates signed by a new key**; changing the embedded public key alone requires a manual client migration.

For local builds, pass signing credentials through a protected environment or private-key path; keep passwords out of shell history. `python3 scripts/release.py prepare --tag vVERSION` stages already-built artifacts. `verify` checks assets, hashes, manifest versions and target matching; it is not a separate cryptographic verifier. Tauri verifies runtime update signatures.
