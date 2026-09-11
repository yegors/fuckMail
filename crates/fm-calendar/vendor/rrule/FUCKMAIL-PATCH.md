# Pinned rrule 0.14.0 with one recurrence correction

Source: crates.io `rrule` 0.14.0, upstream commit `3843456f739d399cfbeff81288c36a66e8e38b83` (`rrule/` in [rust-rrule](https://github.com/fmeringdal/rust-rrule/tree/3843456f739d399cfbeff81288c36a66e8e38b83)). The original registry crate source/examples/manifest/README are retained. The upstream MIT license is included from the same commit; this vendored copy uses its MIT licensing option.

The functional patch is in `src/iter/utils.rs`, `add_time_to_date`. Upstream falls back to adding elapsed time to midnight when a local recurrence time is nonexistent or ambiguous. In Toronto's 2026 spring transition this changes a 02:30 daily recurrence into 03:30 on March 8 and consumes COUNT. [RFC 5545 section 3.3.10](https://www.rfc-editor.org/rfc/rfc5545#section-3.3.10) requires nonexistent recurrence times to be ignored without consuming count. Section 3.3.5 uses the first occurrence when a wall time occurs twice.

The patch uses the timezone's earliest valid local result. Nonexistent candidates return `None` before the iterator decrements count. Ambiguous candidates use their first occurrence. No second recurrence engine or application-side count correction is introduced.

The independent regression in `../../tests/calendar.rs`, `non_existent_dst_wall_time_is_skipped_without_consuming_count`, requires March 7 07:30Z, March 9 06:30Z and March 10 06:30Z for `DTSTART` March 7 02:30 Toronto with `FREQ=DAILY;COUNT=3`. Keep this test when replacing the fork with an upstream release. The existing spring/fall, RDATE/EXDATE, monthly and exception tests protect nearby behavior.

Nonfunctional adjustments remove the now-unused midnight-duration helper and its unused re-export, and suppress upstream missing-documentation warnings that were previously capped for the registry dependency. First-party lints remain enabled. The root workspace excludes this vendored package from first-party package selection.

This is a maintained local patch, not an unmodified upstream dependency. Review the diff when updating. No upstream issue or pull request has been submitted by this task.

Two upstream test expectations are updated for this deliberate RFC correction: the nonexistent local candidate is absent, and the 30th valid daily occurrence moves to March 31. Run the vendored upstream unit suite with `TZ=UTC` because an unrelated upstream validation test assumes the process local timezone is UTC. The root lockfile pins the application; the vendor lockfile pins the independent upstream test dependencies.
