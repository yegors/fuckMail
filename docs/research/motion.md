# Motion

How the GUI moves, and why. The owning contract is [Mail UI](../contracts/mail-ui.md#d205--motion-mail-ui148); the preference is in [Platform](../contracts/platform.md) and [Core](../contracts/core-storage.md).

## October 10: Lasseter's principles applied to the GUI (D205)

**Source.** John Lasseter, "Principles of Traditional Animation Applied to 3D Computer Animation", *Computer Graphics* 21(4), SIGGRAPH '87, pp. 35–44 (Pixar; DOI 10.1145/37401.37407). Read in full from the scanned copy on a UC Irvine course page. The paper adapts eleven Disney principles (from Thomas and Johnston, *The Illusion of Life*) to keyframe animation, illustrated with *Luxo Jr.* and *The Adventures of André and Wally B.*, and closes on personality as the aim of them all.

**What the app had (observed 2026-10-10).** About 20 distinct durations (90 to 1400 ms), all literal, no shared timing tokens and no reduced-motion setting. Several one-shot transitions had no easing curve, so they moved at an even speed (switch knob, toast fade, carets, hovers). The Basic style gives popups, menus, tooltips and dialogs no transitions, so they appeared and vanished in one frame. The reviewed-row fold (170 ms) and the reply and sent-message arrivals (200 ms, 10 px) were the only staged moves. Done, Delay and filing moved the open row to its new section in one frame, the toast only faded, and "Inbox clear" was static.

**Principle → what was built.** Inferences from the paper are ours; the paper is about character animation, not interfaces.

| Principle (paper's sense) | Applied |
| --- | --- |
| Timing: spacing gives weight and meaning; enough time to read an action, no more | One scale in `Theme`: press 90, quick 120, base 170, glide 220, emphasis 420 ms. Leaving is shorter than arriving. Nothing waits on motion: state changes first, motion follows. |
| Slow in and out: inbetweens bunched at the extremes | Arrivals ease out (OutCubic), departures ease in (InCubic), travel eases both ways. The transitions that ran at an even speed now ease. |
| Anticipation: prepare the eye for the action | Main buttons sink 3% and darken while held. An Undo toast draws a line that runs out with the time left to undo. |
| Staging: one idea at a time, unmistakably clear | Done, Delay and filing fold the open row where it was, drifting 18 px toward where it goes (Done and filing left, Delay right), instead of a jump cut. The calendar slides 18 px in the direction of travel. |
| Follow-through and overlapping action: parts settle at different rates; no dead stop before the next action | The toast rises 8 px into place and sinks as it fades. In the Inbox clear moment the title, line and next event start 80, 150 and 220 ms after the check, each before the one above has settled. |
| Arcs: natural paths | Not applied literally: interface travel is short and straight. Direction of travel stands in for it. |
| Squash and stretch: shows material; volume constant; relieves strobing | The pill switch's knob stretches 5 px mid-travel and lands square. The checkbox mark pops in past full size and settles. |
| Exaggeration: go to the essence, balanced against what stays natural | Kept for one moment: reaching Inbox clear through work. The check lands with overshoot and a ring spreads from it. Never on launch or an account switch, and once per clearing. |
| Secondary action: subordinate, before or after the main move, never during it | Account tab counts roll in from the side they moved from after the change lands (down from above as mail clears). Sync's arrows turn while a sync the user would wait on runs. |
| Pose-to-pose vs straight ahead | Popups and dialogs get explicit enter transitions instead of property behaviours. They close at once: an exit fade kept a closed menu on screen long enough to swallow the next key (found by MailSelectionTest). |
| Appeal: simple, readable, nothing clumsy | No bounce anywhere except the two small overshoots. Motion never blocks input: the toast accepts a click from its first frame. |

**Rejected or deferred.** Motion on opening a conversation, switching account or moving with Up/Down: cached navigation has a 100 ms budget and must not animate (Quality). ListView add, remove and displaced transitions: the list lays out each structural batch at once to keep retired delegates from painting over new rows (Mail UI). Undo bringing a row back along its exit path: deferred.

**Reduced motion.** Settings → General → Motion: System (default), Full or Reduced, saved as the `motion` preference. System follows Windows' Animation effects, macOS's Reduce motion and GNOME's `enable-animations`, read at launch and whenever the app returns to the front. Reduced turns every transition into a cut and hides the Undo line and the celebration; looping progress indicators keep their pace. Test, capture and benchmark runs ignore the system setting.

**Evidence.** See D205 in [decisions](../decisions.md).
