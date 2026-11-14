# AUR packages

- `soundhive/` builds the latest tagged release from its GitHub source tarball.
- `soundhive-bin/` repackages the `.deb` of the latest GitHub release, so it
  installs in seconds instead of compiling. x86_64 only, like the release builds.
- `soundhive-git/` builds the latest commit on `main`.

`soundhive` and `soundhive-git` build SoundHive with Tauri's `.deb` bundler and
copy its staged `/usr` tree (binary, desktop file, icons) into the package.
`soundhive-bin` unpacks the same `/usr` tree from the released `.deb`. All three
provide `soundhive` and can't be installed side by side.

Each directory is pushed as-is to its own AUR git repository. The AUR needs
a `.SRCINFO` next to every `PKGBUILD`; regenerate it after each change:

```bash
makepkg --printsrcinfo > .SRCINFO
```

## Releasing a new version

Pushing a stable `vX.Y.Z` tag updates the AUR `soundhive` and `soundhive-bin`
packages automatically: the `aur` job in `.github/workflows/release.yml` runs
once per package, after the GitHub release is published (`soundhive-bin`
downloads its `.deb`). It runs `update-release.sh`, which takes the package's
`PKGBUILD` from this repository, sets the version and checksums, test-builds it
and pushes it to the AUR. Pre-releases (`v0.x`, `-rc1` and so on) are skipped.

`soundhive/PKGBUILD` and `soundhive-bin/PKGBUILD` here are templates, so their
`pkgver` and checksums stay at the version they were last edited for. Change
them here when the packaging itself changes; the job fills in the version.

The job logs in with an SSH key of its own, stored as the repository secret
`AUR_SSH_PRIVATE_KEY`. Its public half is one of the keys on the AUR account,
so it can push to every package of that account.

To update by hand instead, for example if the job failed:

```bash
git clone ssh://aur@aur.archlinux.org/soundhive-bin.git ~/aur/soundhive-bin
packaging/aur/update-release.sh soundhive-bin X.Y.Z ~/aur/soundhive-bin
git -C ~/aur/soundhive-bin push
```

The same works for `soundhive`. Cloning a package that doesn't exist on the AUR
yet gives an empty repository; the first push creates the package.

`soundhive-git` needs no update per release; its `pkgver()` follows the
repository. Push it again only when the `PKGBUILD` itself changes.
