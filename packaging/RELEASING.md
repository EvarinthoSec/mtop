# Release publishing

## Required repository secrets

- `HOMEBREW_TAP_TOKEN`: fine-grained GitHub token with Contents: write access to `EvarinthoSec/homebrew-mtop`. The release workflow updates `Formula/mtop.rb` through the GitHub Contents API.
- `WINGET_TOKEN`: GitHub token usable by `wingetcreate` to open update pull requests against the WinGet community repository. Keep it in Actions secrets; the workflow exposes it through `WINGET_CREATE_GITHUB_TOKEN`, never a command-line argument.

The release itself uses the built-in `GITHUB_TOKEN` with contents write permission. Do not add credentials to the source tree.

## Publish a release

1. Set the workspace package version in `Cargo.toml` and commit the release source.
2. Push an annotated `v<version>` tag. The workflow verifies the tag against the `mtop` Cargo package version.
3. The workflow builds Linux packages, macOS archives and a Windows portable ZIP; asset names include platform, architecture, and version. `SHA256SUMS` covers the published assets.
4. The workflow updates the GitHub release, marks tags containing `alpha`, `beta`, or `snapshot` case-insensitively as prereleases, and clears prerelease status for other tags.
5. When `HOMEBREW_TAP_TOKEN` is configured, it updates the `EvarinthoSec/homebrew-mtop` tap formula. The formula remains attached to the GitHub release as a download option.
6. The first WinGet version is submitted as a community pull request. Once `EvarinthoSec.mtop` exists in `microsoft/winget-pkgs`, later releases use `wingetcreate update --submit`. WinGet acceptance remains subject to upstream validation and review.

To rebuild an already published tag after updating release tooling, run the `Release` workflow manually from `main` and provide `release_tag` (for example `v0.1.0-alpha.1`). This rebuilds the current source, checks that its Cargo version matches, replaces the release assets, and leaves the Git tag unchanged. Use only when source compatibility with the tagged release is confirmed.

## First WinGet submission

The release workflow always creates a versioned WinGet manifest artifact. For the first submission, download that artifact and submit its three YAML files to `microsoft/winget-pkgs` in the canonical `manifests/e/EvarinthoSec/mtop/<version>/` path. After the maintainers merge the package, configure `WINGET_TOKEN` so future releases can submit updates automatically.
