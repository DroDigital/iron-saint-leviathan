# Design notes: inconsistencies found, decisions, improvements

The brief (`docs/DESIGN_PROMPT.md`) is strong but, like any design doc, had gaps and contradictions that only show
up when you build it. This file records each one and what the game does instead. Items marked **[found by tests]**
were discovered by the automated checks (content validation, budgets, the boss balance test, the soak test) rather
than by reading.

## 1. Inconsistencies and gaps in the brief

| # | Brief says | Problem | Resolution |
|---|---|---|---|
| 1 | "Heretic grafts: you may mount **same-side** parts too: a Kaiju part on a Saint" | A Kaiju part on a Saint is the *other* anatomy. "Same-side" only makes sense as "harvested on the side you are playing". | A heretic graft = a part whose anatomy differs from the side the build is played as. Saint builds lose **Sync** (−14 %/part: HUD glitch, control drift, hallucinated enemies); Kaiju builds gain **Machine-blood** (machine parts never regrow). >50 % heretic (4 of 7) unlocks Heretic Form + permanent instability. |
| 2 | Four district states, but "Saint wins push toward Fortified, Kaiju wins toward Flooded" | **Ruined** is not on that axis; nothing said how it is reached or left. | Each district has `control` (−3…+3) **and** persistent destruction. ≥2 = Fortified, ≤−2 = Flooded, else Contested; destruction ≥60 % overrides to **Ruined** (held by nobody). Wins rebuild a little, so Ruined is recoverable. |
| 3 | True ending: "balance the map at exactly 50/50" | The suggested starting map (2 Fortified, 2 Contested, 2 Flooded) is *already* 50/50, so the true ending would be available on turn one. | The Tide Between requires equal Fortified and Flooded counts (≥2 each), total control exactly 0, **and** ≥4 wins on each side. You have to *earn* the balance by playing both discs. |
| 4 | Finales "require particular map states" (no numbers) | Unspecified. | The Breakwater opens after 8 total wins. Drown the City: Kaiju with ≥4 Flooded. The City Stands: Saint with ≥4 Fortified. Become the Saint: Kaiju build >50 % heretic. The Tide Between: rule 3; you fight your best build of the other side driven by your own ghost. |
| 5 | Unlock ST. NULL by severing the heads of the four starting Saints while playing Kaiju | The four starting Saints must therefore exist as *enemies* on the Kaiju disc — the brief never says so. | Each starting chassis is also a district boss on the other disc (Iron Saint at the Harbor, Cinder at the Foundry, Wren at the Rail Yards, Anchor at the Drowned Market; likewise the four Kaiju). The same bosses feed licence rank 5 ("3 wins against that chassis"). |
| 6 | TIDE-WIDOW: "harvest all four starting Kaiju **hearts**" | Hearts/reactors are internal; locational damage can't tear them off in a Sever. | Cores cannot be severed; **every boss always yields its core on death** (and minor enemies can drop one). Harvest = core collected. Recorded per chassis in the Archive. |
| 7 | "Memory card: 15 blocks… campaign saves cost blocks too" + a permanent Archive | If the Archive cost blocks, a full card could make permanent progress unsavable. | The Archive, habit profile and settings live in a **system area** outside the 15 blocks. Blocks hold the campaign (1–3), builds (1–3), the corrupted block (1), runs. A full card never loses progress silently: the save reports what to delete. |
| 8 | Endings: "every Saint part in your collection rusts" | The campaign collection is wiped at war's end anyway, so the penalty would be empty. | It applies to the **builds saved on the memory card**, which persist across wars (and become your nemeses). Rusted: −15 % armour; Crumbling: −15 % HP. |
| 9 | Fuel stamina: "finite each sortie; refills at a hangar" | A long boss fight could hit zero with no recourse – a soft-lock, violating "fair". | Fuel feeds a fast-refilling working tank from a finite reserve; when the reserve is empty the tank still sputters back at 25 % speed. |
| 10 | Hunger: "eat buildings **or units**" | Only buildings were implementable as "eating"; in a sea brawl a Hunger build starved. **[found by tests]** | Every bite of an enemy feeds a little (food and stamina scale with damage dealt); Devour eats building chunks for big refills. |
| 11 | ST. ANCHOR is "tethered to the grid"; blackout weakens it | No mechanic described. | Grid Coupling heart: stamina regen ×1.6 within 45 m of a pylon, ×0.5 in a blackout. Pylons are placed in every district. |
| 12 | Boss telegraphs ≥300 ms | Bosses that spawn minions (Pale Choir verses) bypassed the rule. **[found by tests]** | Verses swing at 0.55× animation speed, attack on a slow cadence and are capped at 4. Validation enforces the 300 ms rule for all phase moves. |
| 13 | "No unavoidable damage" | Unenforced. | Validation rejects dodge-only moves whose active window exceeds the shortest dodge i-frames, and unblockable distance-only moves that reach further than you can outrun. |
| 14 | "Saint parts drop only while playing Kaiju" | Needed a rule that guarantees it, not just a convention. | Drops are always the victim's *own* parts; validation requires every enemy/boss loadout to match its anatomy; the world refuses any drop of the player's own anatomy. Tested by killing everything on both discs. |
| 15 | "Advanced Foundry features: transplanting a **move** between parts" | Moves belong to part *definitions* (anatomy); moving one would make a part lie about what it is. | Transplant moves a **quirk** between parts (quirks are per-instance). Re-roll and transplant unlock at 20 / 40 cross-side harvests. |
| 16 | Assembly "at the Saint hangar" | Changing a build mid-mission would invalidate the running simulation. | The Foundry and Nest are opened between sorties from the war map (they *are* the hangar/trench); resting mid-mission restores, repairs and advances the Tide Clock. |
| 17 | Breakwater seawall district | A continuous seawall sealed Kaiju (who spawn at sea) away from the arena. **[found by tests]** | The breakwater has three sluice gates. |
| 18 | Weather "rolls each Tide" | Undefined cadence. | A tide turns every 6 in-game hours; weather rolls on each turn. Wars begin at dawn. |
| 19 | 8 GB M1 locked 60 fps / 2-hour soak | Cannot be measured on CI hardware. | Headless soak test in CI (`--soak`); profiling steps documented. The final M1 measurement must be done on device. |

## 2. Improvements beyond the brief

* **Tide Runs alternate discs.** You carry one Saint and one Kaiju; each fight is played as the other side, and the
  draft offers parts torn from the enemies you just beat – which fit the build you play next. The roguelite mode is a
  compressed Cross-Harvest loop instead of a single-side gauntlet (pillar 2).
* **Pale Choir HP really is body count.** Splitting transfers HP into verses; killing verses is real damage;
  Recombine only returns what survived.
* **AI fairness is structural.** AI and players share one `Command` type; AI perceives a delayed snapshot; impatient
  AIs react to the nominal windup and so *bite feints* exactly like players do. Nemesis feint behaviour comes from your
  recorded feint-bite rate.
* **Attack cadence counts idle time only.** Every boss attack is followed by a readable punish window. (The first
  implementation let bosses chain swings back-to-back – caught by the balance test.)
* **Habit profiles decay** (×0.8 per mission) so your nemesis evolves with you rather than remembering your first day.
* **Everything is procedural**: no third-party assets at all, so the license gate is trivially green and nothing looks
  like a stock pack (the uniqueness rules are satisfied by construction).
* **Determinism across platforms**: glam's `libm` feature avoids platform-specific `sin/cos`, so Daily Tide seeds and
  bug-report replays match on macOS, Windows and Linux.
* **Saves are crash-safe**: atomic rename + `.bak` auto-fallback + CRC; corrupt files are detected, never half-read.
* **Kaiju swimmers ride the surface** in deep water instead of walking the sea floor.
* **AI stuck detection** (detour sideways when blocked) instead of a navmesh – cheap and good enough for open districts.
* **15-bit everywhere**: world, UI and even the clear colour are 15-bit quantized (the clear colour was not, at first –
  caught by the render test).
* **Input latching**: one-shot presses survive frames that run zero simulation ticks (fixed-timestep jitter would
  otherwise eat a parry press).

## 3. Known gaps / honest status

* Greybox art: chassis are 450–650 triangles today (brief: 800–1,500); the budget gate allows the headroom for final art.
* No navmesh: AI routes straight with detours; dense blocks can still delay an enemy.
* Kaiju animation is procedural but simple (trail-following spines, flapping, swarm offsets); no IK foot placement yet.
* Music is procedurally composed per district; hand-written tracker modules can replace it later through the same
  five-layer interface.
* The M1 60 fps target, controller rumble on real hardware and a 2-hour on-device soak need physical devices.
* `docs/gen_content_bootstrap.py` generated the initial RON content once; the RON files are now the source of truth
  (re-running the script would overwrite hand edits).
