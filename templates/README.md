# Reference Projects

These projects are the packaging contract for `cargo-tizen`.

- `reference-projects/rpm-app` shows the standard RPM layout at `tizen/rpm/<cargo-package-name>.spec`.
- `reference-projects/tpk-service-app` shows the standard TPK layout at `tizen/tpk/tizen-manifest.xml`, plus optional `reference/` and `extra/` directories.

The codebase tests read these projects directly, so they are both user-facing examples and regression fixtures.

