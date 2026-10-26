# RFC-0032 slice C2: the stored-patch study (protocol)

Question (RFC-0032 §3.2, §10): **when a user removes something and accepts the result, should the filled area be a stored result that never changes, a result that is re-made when the photo is opened, or a stored result that can be re-made on request?**

The measurements in §10 say the technical risk is smaller than assumed (a re-run on the same machine is identical; across providers only fine texture differs). This study asks what people expect and trust, which numbers cannot.

## Material

- The mock: `docs/ux/mockups/remove-tool-mockup.html`, replay 6 ("Reopen the photo (C2)"). Two selectors: **Patch** A (stored, never changes) and B (re-runs when the photo is opened, the fill changes slightly), and the buttons **Close and reopen photo** and **Patch file missing**.
- A third condition, **C (hybrid)**, is not in the mock yet: stored, and when the file is missing the entry offers one **Regenerate** button that says the result may differ slightly. In the mock this is what "Patch file missing" followed by *Regenerate* already does; the wording to test is below.
- 5 to 8 photographers; the project owner first (a dry run). Mixed: some who use Lightroom's Remove or Photoshop's Generative Fill, some who do not. 15 minutes each, in person or on a call, screen shared, think aloud. Nothing is recorded without consent; notes only.

## Tasks (same order for everyone, conditions rotated)

1. **Remove and accept.** "Remove the person and the bin, accept it." (A and B the same here.)
2. **Come back.** "Close the photo and open it again." Variant A: identical. Variant B: the filled areas change slightly. *Ask: what did you see? Was that what you expected? Would you trust it before sending this to a client?*
3. **Edit further, then come back.** "Change the exposure, then close and reopen." Same ask.
4. **Copy the catalog to another computer.** (Mock: **Patch file missing**.) Variant A with no file: "needs regenerating", nothing happens by itself. Hybrid: the same plus a **Regenerate** button. *Ask: what do you think that means? What would you press? What do you expect to happen to the picture you accepted?*
5. **The choice.** Describe the three in one sentence each (plain wording below) and ask which they would want, and what would make them change their mind.

Plain wording for task 5:

- **A, stored:** "Your accepted removal is saved exactly as it was. It never changes."
- **B, re-made:** "Your removal is re-made each time you open the photo. It looks the same in nearly every case, but the filled area can change slightly."
- **C, stored and re-makeable:** "Your accepted removal is saved exactly. If the saved copy is ever lost, you can re-make it with one button, and it may look slightly different."

## What to record per person

- Noticed the change in B (yes / no, how long it took).
- Reaction to the change: surprise, indifference, annoyance, trust lost (free words).
- Understanding of "needs regenerating" (their own words) and what they pressed.
- Choice A / B / C, and why, and the one thing that would change it.
- Anything about Remove itself (marking, the result bar, Heal as the fallback, the large-region guard) that they stumbled on: this feeds slices D and E.

## Decision rule (fixed before the study)

- If a majority choose **C or A** and none say a silent change would be acceptable: build **C** (stored file, recomputable on request, never silent). This is the hypothesis in RFC-0032 §10.
- If most choose **B** because "it looks the same anyway" and none would notice the change: still build C (the design is cheaper to reason about and carries over to diffusion fill, §10), and note that B was acceptable.
- If anyone loses trust on seeing the change in B: **B is out**, whatever the others say.
- Fewer than five people is a signal, not a result: report it as that.

## After

Record the answers in RFC-0032 §10 (names omitted), update §3.2 to the decision, and move slice D forward. The study and the licence step (§8 B) are the two gates before the engine is built.
