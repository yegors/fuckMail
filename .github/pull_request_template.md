**What bug does this fix?**
<!-- Steps to reproduce, or a link to the issue. fuckMail takes bug fixes; for new features or other providers, please fork. -->

**How did you verify it?**
<!-- A test that fails before and passes after is ideal. Mention your OS. -->

- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` are clean
- [ ] `scripts/dev.sh test --preset release` (or `./scripts/dev.ps1 test --preset release`) passes
- [ ] No credentials, real mail data or generated artifacts are included
