from pathlib import Path

root = Path(__file__).resolve().parents[3]
common = "P2-05B3B implementation is active under D033 from B3A functional interim `efe66b6`. Its new behavior is not yet verified; the immutable B3A archive retains the previous scoped passes. D031 still defers performance measurement. Whole readiness stays 0/9."
contracts = {
    "core-storage": (22, "Revision 22 uses schema 16 and the existing Store/drafts/blob/capture structures for explicit source conversion, observed-target adoption and typed remote-draft deletion. Bounded twin metadata retains derivation separately from adoption and acknowledged upload baselines; indexed due selection excludes an unchanged newly adopted revision. Both upload and Send use one effective target rule. Fresh source checks, opaque off-writer content proofs and short revision-fenced commits preserve original RAW, authored edits and old receipts. Delete guards block conflicting exact-target work; kind `draft_delete` is excluded from upload/Send/synthetic loaders and inventory owner linking. Missing provider state never proves sending or an uncertain mutation's success. Consumers: Gmail 11/Mail UI 20/CLI 15/Quality 18; Calendar 15/Platform 12/Search 5, fixtures 0/3 and protocol 1 stay unchanged."),
    "gmail": (11, "Revision 11 adds explicit remote-draft authoring and deletion under D033. A plain-text copy follows ordinary local autosync; adoption requires a fresh exact-target RAW check but performs no initial upload. The first edit and immediate explicit Send retain the adopted container without inventing an ACK. Same-target editing initially accepts only completely representable plain-text MIME and ordinary attachments; unsupported semantics remain inspectable or explicitly convertible. Remote-draft deletion uses a distinct journal, exact verb/path quota admission, fresh retained source check and one claimed DELETE. Definite outcomes and uncertainty persist through cancellation/disconnect; explicit reconciliation only reads and never retries. Received-mail permanent deletion remains unavailable. Consumers: Core 22/Mail UI 20/CLI 15/Quality 18/schema 16; fixtures 0/3/protocol 1 unchanged."),
    "mail-ui": (20, "Revision 20 adds an Actions menu beside Saved sources in the existing Drafts preview. Create plain-text copy confirms primary From, retained recipients/files, omitted HTML/inline placement/thread linkage, no added signature and normal autosync. Edit Google draft requires Core's full-source fidelity assessment and a fresh source check; adoption alone does not upload. Delete Google draft permanently is separate from local Discard and identifies the exact account/source. Retained receipts expose definitive, refused and uncertain outcomes plus read-only Check Google recovery; absence never means sent or confirmed deleted. Confirmations and replies retain account/source/navigation stamps, while original source/export fences and three-theme geometry remain intact. Consumers: Core 22/Gmail 11/CLI 15/Quality 18/schema 16; fixtures 0/3/protocol 1 unchanged."),
    "cli": (15, "Revision 15 pairs exact-source authoring assessment, explicit plain-text copy, supported adoption and remote-draft deletion/recovery with the native UI. Every command requires explicit profile/account scope; identifiers, source revisions, conversion choices and request IDs are validated before connecting. Assessment/copy and retained status remain local; adoption, deletion and reconciliation require an owned Google session. Copy uses explicit primary/plain_text/attachments/omit policies and normal autosync. Exact retries preserve their original receipt and never manufacture mutation certainty. The executable grammar is recorded in the local protocol reference as implementation lands. Consumers: Core 22/Gmail 11/Mail UI 20/Quality 18/schema 16; fixtures 0/3/protocol 1 unchanged."),
    "quality": (18, "Revision 18 records D033's B3B functional gates: schema-15 upgrade/refusal and receipt preservation; complete copy routing/text/file identity and unsupported conversion refusals; no upload on adoption, correct first-edit and immediate-Send target; stale/cross-account source/confirmation fences; distinct delete admission and journal replay, changed target, known failure/lost response/cancellation/restart; and native/CLI receipt/source inspection. Exact known-target unresolved work blocks conflicts; similarity never establishes ownership. Generated providers supply independent write/read and MIME oracles. D031 defers benchmark execution, not the existing latency/resize requirements. Current packages/CI, complete Calendar and protected real-account admission remain separate. Consumers: Core 22/Gmail 11/Mail UI 20/CLI 15/schema 16; fixtures 0/3/protocol 1 unchanged."),
}
for name, (revision, text) in contracts.items():
    path = root / "docs/contracts" / f"{name}.md"
    lines = path.read_text(encoding="utf-8").splitlines()
    states = [i for i, line in enumerate(lines) if line.startswith("State: Draft · Revision:")]
    assert len(states) == 1, path
    index = states[0]
    lines[index] = f"State: Draft · Revision: {revision} · Updated: 2026-09-13 · Implementation: {common}"
    if text not in lines:
        lines[index + 1:index + 1] = ["", text]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")

packet = "**Active packet: P2-05B3B under D033**, from B3A functional interim `efe66b6`: explicit plain-text copies, supported same-target adoption/editing and journalled remote-draft deletion/recovery. Core 22/Gmail 11/Mail UI 20/CLI 15/Quality 18 define schema 16 in the existing Store; Calendar 15/Platform 12/Search 5, fixtures 0/3/protocol 1 stay unchanged. Implementation is underway, with no new acceptance claimed. B3A's source-specific evidence is preserved below. Performance measurement remains deferred under D031; complete Calendar, packages/CI and actual-account gates remain required."
for name in ["docs/status.md", "docs/plans/phase-2.md"]:
    path = root / name
    value = path.read_text(encoding="utf-8")
    first, remainder = value.split("\n", 1)
    path.write_text(first + "\n\n" + packet + "\n" + remainder, encoding="utf-8")

path = root / "docs/status.md"
value = path.read_text(encoding="utf-8")
begin = value.index("**Continue B3 remote Drafts from the B2 functional interim checkpoint:**")
end = value.index("\n\n", begin)
value = value[:begin] + "**Continue B3B from `efe66b6`:** D033 starts explicit source conversion, supported observed-target editing and remote-draft deletion/recovery. Pair the native/CLI paths and verify their source, receipt, lifetime and upgrade gates. Then complete remaining Calendar invitations, recurrence and running-app reminders. Preserve B2's failed performance result and deferred measurement under D031; no real account or external write has been exercised." + value[end:]
path.write_text(value, encoding="utf-8")

path = root / "README.md"
value = path.read_text(encoding="utf-8")
needle = "## Current state\n"
assert value.count(needle) == 1
path.write_text(value.replace(needle, needle + "\n" + packet + "\n", 1), encoding="utf-8")

path = root / "AGENTS.md"
value = path.read_text(encoding="utf-8")
needle = "## Current stage rules\n"
assert value.count(needle) == 1
path.write_text(value.replace(needle, needle + "\n- D033 starts P2-05B3B from B3A functional interim `efe66b6`: explicit plain-text copy, supported observed-target adoption and typed remote-draft deletion. Follow Core 22/Gmail 11/Mail UI 20/CLI 15/Quality 18/schema 16. Adoption alone must not upload; preserve source/receipt identity and unknown outcomes. D031 still defers performance measurement. No live actions are authorized by the packet.\n", 1), encoding="utf-8")

path = root / "docs/research/phase-2-execution.md"
value = path.read_text(encoding="utf-8")
value += """
## P2-05B3B explicit remote-draft actions under D033

Started 2026-09-13 from `efe66b6`, the clean B3A functional interim checkpoint. Core 22/Gmail 11/Mail UI 20/CLI 15/Quality 18/schema 16 scope the next implementation. Original source snapshots remain read-only; new explicit copy/adoption/deletion paths use the existing store and journal. No B3B check has passed yet. D031 defers performance runs; the earlier failed navigation evidence is unchanged.

Primary API review: Google's [draft deletion reference](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/delete) specifies immediate permanent deletion and a successful empty response object. Its [draft update reference](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/update) describes content replacement; the reviewed method exposes no revision-precondition parameter. Therefore fetch-then-write is treated as a remaining race, not an atomic precondition guarantee. The [draft guide](https://developers.google.com/workspace/gmail/api/guides/drafts) distinguishes the stable container from replaceable contained-message identity and describes consumption on Send. A later missing container cannot establish which action caused it.

Code inspection found two integration requirements before authoring: B2's baseline-only target selection would otherwise directly send an adopted draft before its first upload ACK, and quota class/cost alone cannot distinguish create from same-cost deletion. D033 requires one observed-or-acknowledged target rule and exact delete verb/path admission. Full-source preparation must also enforce local routing/composer limits rather than relying on B3A's bounded preview. Functional evidence will be appended here after independent fixtures, native/CLI workflows, migrations and lifecycle checks run; no live-account or new package/performance acceptance is implied.
"""
path.write_text(value, encoding="utf-8")
print("Updated B3B owning contracts, active status and execution start packet.")
