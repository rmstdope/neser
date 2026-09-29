# nr-z3u — retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-09-29
- **PR:** #3316

## The first fix merged green, but games 1–3 still showed a black screen

**What happened.** PR #3308 fixed a real difference between NESER and Mesen2 in mapper 83: the DBZ $B000 aliases had turned on 2 KiB CHR banking. It merged with unit tests only, and the navigator's verification at a5daca91 failed because games 1–3 still went black. Selecting each game headlessly with a small probe binary showed the real cause: games 1–3 hang without PRG-RAM at $6000–$7FFF. NESdev's INES Mapper 083 says board 83.2 always carries 32 KiB of PRG-NVRAM there, and Mesen2 maps it only through its game database (MesenNesDB: 32 KiB save RAM, submapper 2).
**Why.** The first fix compared the mapper's register code with Mesen2's `Mapper83.h`. It never ran the failing ROM, so it could not see that Mesen2's behaviour for this cart also comes from the database-driven default work RAM in `BaseMapper`, not only from the mapper file.
**Cost.** A second bugfix pass, a failed verification, and a P0 reopen.
**Prevent by.** In `fix-bug`, step 3 ("make the reproduction test fail for the right reason"), add: when the bug is a game that misbehaves, first confirm the cause by running the ROM with the needed input, before writing the unit test. The throwaway harness is a few lines using `Nes::set_joypad_button_states` and `run_one_frame_discarding_audio`. A patched mapper that makes the real ROM behave proves the cause; a matching diff against Mesen2 does not.
**Seen before.** None found.
