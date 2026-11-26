# Releasing

1. Bump `version` in `Cargo.toml` in a pull request, with `Cargo.lock` updated, and merge it once CI is green.
2. Tag the merged commit on `main` with `v` and that version, and push the tag:

   ```sh
   git switch main && git pull
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   `gh release create vX.Y.Z --target main --generate-notes` does the same and adds a GitHub release.
3. The tag starts [`publish.yml`](.github/workflows/publish.yml), which publishes the crate to crates.io.

`publish.yml` uses crates.io trusted publishing: no token is stored in the repository.
It runs in the `crates-io` environment, limited to `v*` tags, and trades the job's OIDC token for a short-lived crates.io token.
It fails unless the tag is `v` and the version in `Cargo.toml`, then runs `cargo publish --locked`.
A version is published once: to fix a bad release, bump the version and tag again.
