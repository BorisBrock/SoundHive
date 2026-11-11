# Releasing SoundHive

Releases are built and published by GitHub Actions.

| Workflow | Runs on | Does |
|---|---|---|
| [`ci.yml`](../.github/workflows/ci.yml) | every push to `main` and every pull request | Prettier, svelte-check, frontend and Rust tests, clippy, and a build on Linux and Windows |
| [`build.yml`](../.github/workflows/build.yml) | the same, plus by hand and from `release.yml` | builds all packages and keeps them as workflow artifacts |
| [`release.yml`](../.github/workflows/release.yml) | tags like `v1.2.0` | builds all packages, publishes the GitHub release and updates the AUR package |

## Publishing a release

The release tag is the only place the version is set. The version in `package.json`,
`src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` stays at `0.0.0` in the repository.

Tag the commit you want to release and push the tag:

```bash
git tag v1.2.0
git push origin v1.2.0
```

The release workflow reads the version from the tag and writes it into those files with
[`tools/set-version.sh`](../tools/set-version.sh) before building. It builds the packages on
Windows and Ubuntu 24.04 and attaches them to a GitHub release named `1.2.0`, with release notes
generated from the merged pull requests and commits.

| Platform | Files |
|---|---|
| Windows | `SoundHive_<version>_x64-setup.exe` (installer), `SoundHive_<version>_x64_en-US.msi`, `SoundHive-<version>-windows-x64.exe` (portable) and `.zip` |
| Linux | `SoundHive-<version>-linux-x64.AppImage`, `SoundHive_<version>_amd64.deb`, `SoundHive-<version>-linux-x64.tar.gz` |

Tags like `v1.2.0-rc1` and all `v0.x` versions become pre-releases and do not update the AUR.

## AUR

Stable releases also update the [`soundhive`](https://aur.archlinux.org/packages/soundhive) AUR
package, which builds from the release's source tarball. The job needs the repository secret
`AUR_SSH_PRIVATE_KEY`. See [`packaging/aur/README.md`](../packaging/aur/README.md) for details and
for updating the package by hand.
