# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: open the repository's
**Security** tab and choose **Report a vulnerability**. Do not open a public
issue. You can expect an acknowledgement within a week.

Only the latest release is supported; fixes are released as a new version.

## Verifying a release

Every release is built and published by
[`.github/workflows/release.yml`](.github/workflows/release.yml) from a tag on
`main`; nothing is uploaded by hand.

- **GitHub release archives** carry build provenance attestations:

  ```sh
  gh attestation verify oasis-gen-x86_64-unknown-linux-musl.tar.gz --repo 22mb/oasis
  ```

  `SHA256SUMS` lists the checksum of every archive (`sha256sum -c SHA256SUMS`).
  Releases are published only after all assets are attached, so they can be
  made immutable.

- **npm packages** (`oasis-gen` and `@oasis-gen/*`) are published with npm
  provenance, linking each version to the workflow run that built it. Check the
  installed packages with:

  ```sh
  npm audit signatures
  ```

## Supply chain

- GitHub Actions are locked to commits (and repository IDs) in
  [`.github/workflows/actions.lock`](.github/workflows/actions.lock), verified
  in CI with `gh actions-lock --verify`.
- Workflows run with no permissions by default and grant the minimum per job;
  checkouts do not persist credentials, and publishing uses no caches.
- The release job runs in the `release` environment and refuses tags that are
  not on `main` or do not match the version in `Cargo.toml`.
- The Rust toolchain is pinned in `rust-toolchain.toml`, builds use
  `Cargo.lock` (`--locked`), and `cargo-deny` checks advisories, licenses and
  sources. The test dependencies are locked in `tests/ts/package-lock.json`.
- Dependabot proposes updates only after a 7-day cooldown.
- The npm packages have no dependencies besides the platform binaries.
