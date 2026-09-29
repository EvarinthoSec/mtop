# Release publishing

## Required repository secrets

- `HOMEBREW_REPO_TOKEN`: fine-grained GitHub token with Contents: write access to `EvarinthoSec/homebrew-repo`. The release workflow updates `Formula/mtop.rb` in the dedicated tap.
- `WINGET_TOKEN`: GitHub token usable by `wingetcreate` to open update pull requests against the WinGet community repository. Keep it in Actions secrets; the workflow exposes it through `WINGET_CREATE_GITHUB_TOKEN`, never a command-line argument.

The release itself uses the built-in `GITHUB_TOKEN` with contents write permission. Do not add credentials to the source tree.

## Publish a release

1. Set the workspace package version in `Cargo.toml` and commit the release source.
2. Push an annotated `v<version>` tag. The workflow verifies the tag against the `mtop` Cargo package version.
3. The workflow builds Linux packages, macOS archives and a Windows portable ZIP; asset names include platform, architecture, and version. `SHA256SUMS` covers the published assets.
4. The workflow updates the GitHub release, marks tags containing `alpha`, `beta`, or `snapshot` case-insensitively as prereleases, and clears prerelease status for other tags.
5. When `HOMEBREW_REPO_TOKEN` is configured, it updates `Formula/mtop.rb` in `EvarinthoSec/homebrew-repo`; the same formula is also attached to the GitHub release. If the token is absent, the release formula is still published but the tap update is skipped.
6. The first WinGet version is submitted as a community pull request. Once `EvarinthoSec.mtop` exists in `microsoft/winget-pkgs`, later releases use `wingetcreate update --submit`. WinGet acceptance remains subject to upstream validation and review.

To rebuild an already published tag after updating release tooling, run the `Release` workflow manually from `main` and provide `release_tag` (for example `v1.0.0`). This rebuilds the current source, checks that its Cargo version matches, replaces the release assets, and leaves the Git tag unchanged. Use only when source compatibility with the tagged release is confirmed.

## First WinGet submission

The release workflow always creates a versioned WinGet manifest artifact. For the first submission, download that artifact and submit its three YAML files to `microsoft/winget-pkgs` in the canonical `manifests/e/EvarinthoSec/mtop/<version>/` path. After the maintainers merge the package, configure `WINGET_TOKEN` so future releases can submit updates automatically.

## AUR package (`mtop-bin`)

The AUR recipe is maintained in `packaging/aur/PKGBUILD` with its generated `.SRCINFO`. It repackages the prebuilt Arch release asset and intentionally conflicts with the unrelated AUR package named `mtop`, because both install `/usr/bin/mtop`.

For each release, update `pkgver` and the Arch asset's SHA-256, regenerate `.SRCINFO` with `makepkg --printsrcinfo`, then build and smoke-test the package in an Arch environment. Submit both files to the `mtop-bin` AUR Git repository using an SSH key registered with AUR. The command `yay -S mtop-bin` becomes available after AUR accepts the submission.

## Launchpad PPA (`ppa:evarinthosec/ppa`)

The `Launchpad PPA` workflow creates a vendored Debian source package for Ubuntu resolute (26.04) and uploads it after the matching OpenPGP key is configured. Launchpad builds the binary package `mtop-bin` from source; the `.deb` attached to a GitHub Release is not uploaded directly. Launchpad builds offline with the archive toolchain (resolute ships Rust/Cargo 1.93), so the workspace `rust-version` and every locked dependency must stay at or below that; `sysinfo` is held at 0.38.x because 0.39 requires Rust 1.95. Tags cut before this pin (including `v1.0.0`) cannot be built by Launchpad. Ubuntu noble's archive Rust/Cargo 1.75 is too old. Continue using the direct release `.deb` on other supported Ubuntu versions.

Before the first upload, register the uploader's public OpenPGP key and SSH public key with the Launchpad account, then configure these GitHub Actions values under Settings → Secrets and variables → Actions:

- Secret `LAUNCHPAD_GPG_PRIVATE_KEY`: ASCII-armored private key corresponding to the registered public key.
- Secret `LAUNCHPAD_GPG_PASSPHRASE`: key passphrase, if the OpenPGP key is protected.
- Secret `LAUNCHPAD_SSH_PRIVATE_KEY`: private key corresponding to the registered SSH key.
- Secret `LAUNCHPAD_SSH_PASSPHRASE`: key passphrase, if the SSH key is protected.
- Variable `LAUNCHPAD_GPG_KEY_ID`: full fingerprint of that registered key.

Trigger the workflow manually for an existing stable tag to publish the initial PPA version. Later stable GitHub Releases trigger it automatically. A successful `dput` only means the source upload was submitted; verify Launchpad's build and publication status before telling users to install with `apt`. After Launchpad publishes the build on Ubuntu 26.04, install with `sudo add-apt-repository ppa:evarinthosec/ppa`, `sudo apt update`, then `sudo apt install mtop-bin`.
