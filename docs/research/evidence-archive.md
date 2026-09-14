# Evidence storage and September 14 cleanup

D046 moves generated execution evidence out of the source tree. Git retains authored reports, decisions, contracts, reproducible test code and fixtures, and the 17 supplied design-reference assets. Raw logs, traces, screenshots, inventories, manifests and packaged run archives belong in ignored local storage or CI artifacts. Historical results, failures and source/binary identities remain valid only within their recorded scope.

## Preserved snapshot

Before removing any originals, the cleanup archived all three `docs/research/assets/phase-0`, `phase-1` and `phase-2` directories and read every member back, checking exact bytes and SHA-256 against the original inventory.

| Snapshot | Value |
| --- | --- |
| Local archive, relative to repository root | `.artifacts/evidence/2026-09-14-pre-cleanup.tar.gz` |
| Archive size | 169,759,826 bytes |
| SHA-256 | `8407fff9bfb20cb2cc9e1b78a3ae9d269899f369bfa5e12485047bae05cf8b3d` |
| Original files | 2,796 (2,661 tracked; 135 uncommitted Mac continuation files) |
| Original file bytes | 243,213,470 |
| Internal inventory | `EVIDENCE-SNAPSHOT.json`, with original paths, sizes, SHA-256 and tracked status |

The snapshot includes the immutable Windows 997-artifact ZIP unchanged, earlier failed/interrupted attempts, and the single Mac continuation directory. Local profiles, credentials, SDKs and private mailbox notes were not included. This is a local archive, not an off-machine backup; the 135 Mac files have no historical Git copy. Transfer this archive separately when moving hosts.

Previously committed evidence remains available at these existing commits:

- [Merged Phase 0/1 assets at `885de67`](https://github.com/yegors/fuckMail/tree/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets).
- [Complete pre-cleanup tracked assets at `8155e99`](https://github.com/yegors/fuckMail/tree/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets), including Phase 2 and the later Phase 1 merge-equivalence record.

Reports now link to exact historical files at those commits. Each of the 340 replaced links was checked against the Git object and original local bytes. Inline paths in historical prose and inside old manifests remain original archive member paths; they do not require restoring generated files into the current checkout. New Mac evidence is available from the local snapshot only.

## Recover evidence without adding it to Git

From the repository root, verify and extract the local snapshot into ignored storage:

```sh
shasum -a 256 .artifacts/evidence/2026-09-14-pre-cleanup.tar.gz
mkdir -p .artifacts/restored
tar -xzf .artifacts/evidence/2026-09-14-pre-cleanup.tar.gz -C .artifacts/restored
```

Original paths then appear beneath `.artifacts/restored/docs/research/assets/`. The extracted `EVIDENCE-SNAPSHOT.json` supplies per-file checksums. On Windows, `Get-FileHash -Algorithm SHA256` checks the archive; the same `tar` extraction works with an existing destination directory.

A fresh checkout with the historical commit can recover any previously tracked artifact, for example:

```sh
git show 8155e998d5957e60c491475df7877553db2b453b:docs/research/assets/phase-2/p2-final/manifest.json
```

This cleanup does not rewrite history or shrink existing Git objects. A squash merge keeps the new Phase 2 raw evidence out of the resulting main commit. Preserve access to the old commits or arrange an off-repository archive before separately pruning history; especially preserve the local-only Mac evidence.

## Future runs and checks

Write raw output beneath ignored `.artifacts/`, `.tools/`, `build/` or `dist/`; the existing native CI workflow uploads logs and packages as Actions artifacts. Keep concise measured results, failures, source scope and artifact locations/checksums in the owning research report. Do not copy bulk output back into documentation or treat artifact retention as permanent backup. Never archive credentials or real mailbox dumps for publication.

Run `python3 scripts/check_repo_hygiene.py` before committing. Native CI runs it before building on all three hosts. It rejects generated output paths (including forced additions), raw log/archive/database files under `docs/`, individual files over 2 MiB and curated documentation assets totaling over 10 MiB. Ignored local artifacts and deleted files do not count. The retained design references total 4,275,043 bytes; runtime fixtures remain ordinary source inputs. Any future size exception needs an explicit, narrow policy change.

## Cleanup verification

- Removed 2,661 tracked generated files after verifying all 2,796 archived originals; retained all 17 design files unchanged. Documentation assets fall from 247,488,513 to 4,275,043 bytes.
- The tracked working-tree diff against merged Phase 1 `885de67` falls from 1,116,037 to 88,675 added lines (about 92% fewer). Both counts exclude untracked files; the new guard/archive index and previously untracked app sources are small additional changes. Historical Phase 0/1 removals also appear as deletions against main.
- All 393 existing application, fixture and tooling inputs checked before cleanup remain byte-identical, including the three uncommitted OAuth/setup files. Earlier decision entries remain an exact prefix of the append-only log.
- The hygiene check passes locally. Seven disposable-repository scenarios pass: ordinary fixtures, ignored artifacts, forced artifact refusal, prospective deletion, raw documentation-log refusal, individual size refusal and aggregate asset-size refusal. The check is wired into native CI; hosted CI was not rerun for this cleanup.
- All 340 retired-asset links resolve to verified historical bytes; five older host-local build links now point to their archived records. Markdown file destinations and `git diff --check` pass. Application tests were not rerun for this documentation/storage change.
