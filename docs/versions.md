# Sudo for Windows - Versions

There are a few different versions of Sudo for Windows - this doc aims to
outline the differences between them. Each includes different sets of code, and
releases in different cadences.

* **"Inbox"**: This is the version of Sudo that ships with Windows itself. This
  is the most stable version, and might only include a subset of the features in
  the source code. This is delivered with the OS, via servicing upgrades.
  - Build this version with `cargo build --no-default-features --features Inbox`
* **"Stable"**: The stable version of Sudo for Windows which ships out of this
  repo. This can be installed side-by-side with the inbox version.
  - Build this version with `cargo build --no-default-features --features Stable`
* **"Dev"**: This is a local-only build of sudo. This has all the bits of code
  turned on, for the most up-to-date version of the code.
  - Build this version with `cargo build`

Dev builds are the default for local compilation, to make the development inner loop the
easiest.

For more info, see "[Contributing code](../CONTRIBUTING.md#Contributing-code)" in
the contributors guide.
