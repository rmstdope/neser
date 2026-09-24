# Instructions for the fleet

This repository is developed partly using Cerebro (`.cerebro/cerebro`) and
steered by one person, the navigator. The navigator only makes technical decisions on architectural
level. Details like files, tests and approach are the agents' to decide. The navigator ranks the work,
agrees every user-visible experience before it is built, and verifies it once it has merged. When a
person drives a session by hand, the same rules apply: they are the navigator, the session is the
driver, and the driver still makes suggestions for improvements and fixes.

## The project

Neser is a multi-system retro console emulator written in Rust: NES/Famicom, Game Boy, Game Boy
Color, Game Boy Advance and Super Nintendo cores behind one platform layer, with a native desktop
frontend and a WebAssembly frontend served from `web/`. Players use it to run their game ROMs and
the well-known hardware test ROMs; developers use its headless capture and mapper verification
tooling to compare behaviour against the hardware specifications on nesdev.org, Pan Docs, GBATek
and fullsnes. "Working" means a game or test ROM behaves as it does on the real console: the test
ROM suites under `roms/` pass, golden frames match, and nothing regresses in the pre-merge gate.

## Skills Usage

Always select the appropriate skill for a specific task. Be sure to ALWAYS explicitly write in the
chat what skills that are currently being used. Always follow the instructions in the skills to the
letter. The hardware research skills (`nes-hardware-research`, `gb-hardware-research`,
`gba-hardware-research`, `snes-hardware-research`) are the way to answer any question about what
the hardware does; `mapper-verification-roms` governs every verification ROM.

## Producer review

Nothing merges red. Before delivery, a producer obtains and addresses one independent, full review
of the complete diff and bead. If its changes are substantial enough to make another review useful,
the producer chooses the right follow-up scope and obtains it; minor, self-contained answers need
not create a review loop. Unresolved findings, a red or missing check, or a reviewer that cannot
produce a usable result go to a person.

Every pull request runs the full gate in `scripts/gate-full.sh` (see "Committing and Merging to
main" below) before it is opened, and is judged by CI on the same commands. Even a failure that
existed before the change is fixed before merging; nothing with a known red check lands on main.

## Work tracking

*Read by every role through `skills/beads-workflow`, which carries the commands; this section is
where a project says anything that differs.*

Planned work is tracked in beads (`bd`, prefix `nr`). GitHub issues are the inbox for outside
requests and bug reports only: Moira, the user-feedback agent, triages each new issue with the
navigator into a bead (linked by `external_ref gh-<n>`), keeps the issue's status comments in step
with its bead, and closes the issue when the work has shipped. Nobody files planned work as a
GitHub issue. Every bead is created unranked (P4) and ranked later with the navigator; a bead is
planned in one session and implemented in another.

What differs here from `beads-workflow`:

- **Labels carry the area**, using the vocabulary the GitHub issues used: `nes`, `gb`, `cgb`,
  `gba`, `snes`, `mapper`, `games`, `platform`, `web`, `refactoring`, `testing`. Put at least one
  area label on every bead so `bd list -l snes` answers "what is open for the SNES".
- **Mapper beads.** A bead that adds or verifies a mapper is built against the specification on
  https://www.nesdev.org/wiki/Mapper (fallback: https://nesdev-wiki.nes.science/). When writing a
  verification ROM under `roms/automated_tests/mapper_verification/`, NEVER read the implementation
  of the mapper under test; the ROM must follow the specification alone. Cover every submapper.
- **Retrospectives.** When something surprised the producer (a trap, a wrong assumption, a tool
  that misbehaved), write `docs/retrospectives/<bead-id>.md` in the same PR and, if a skill should
  change because of it, change the skill in that PR too.
- **The board syncs through the Dolt remote, not git.** No `.beads/*.jsonl` is tracked. A fresh
  clone runs `bd bootstrap` (which refuses if a database already exists, so do not run `bd list`
  first); after that it is `bd dolt pull` and `bd dolt push`. There is no `bd sync`.
- **Git hooks.** `core.hooksPath` points at `.githooks`, whose hooks auto-format staged Rust and
  Python and then forward to the bd hooks under `.beads/hooks`. Running `bd init` again repoints
  `core.hooksPath`; set it back to `.githooks`.
- **Migrated issues.** The 17 issues that were open on 2026-09-24 became beads with
  `external_ref gh-<n>`; the table is in `docs/migration/github-to-beads.md`. Closed issues before
  that date stay on GitHub as history.

## Development practices

*Read by planners and producers when deciding how much to build at once and how to test it.*

### Small increments

The application shall ALWAYS be developed in very small, manageable increments that can be delivered
independently. Each increment should add a specific feature or improvement to the application. When a
bead is larger than one increment, split it into child beads rather than growing it. The code base
should ALWAYS have a great safety net of tests to ensure that new changes do not break existing
functionality.

### Test-driven Development (TDD)

Code is written test-first. This should be the case also when fixing bugs. First write a test that
reproduces the bug, then fix the bug and verify that the test passes along with all existing tests.
However, when trying to pinpoint a bug, you are free to add any traces, try fixes or anything else
without having to write tests for that immediately. But once the issue has been pinpointed, either
update existing tests or add a new test that triggers the error before applying the fix. This
ensures no unnecessary modifications are done and helps to prevent regressions in the future.

### Collaboration

Interactive sessions collaborate closely with the navigator to ensure that the application meets
their needs and expectations. The navigator decides what players will see and in which order things
get built; the session decides how. If at any time there are uncertainties or ambiguities, the
session seeks clarification from the navigator using the question UI/tool with predefined answers
when possible, and free text options when necessary. A session running unattended that hits a
decision it must not make escalates the bead to the navigator as `beads-workflow` describes, rather
than guessing.

### Design

Always prefer simple design solutions. Avoid over-engineering; say so when you decline a more
general one. If unsure, ask the navigator for clarification. The design should be easy to change if
need be.

### Branches, commits and pull requests

Follow `beads-workflow`: one bead per branch named `<bead-id>-short-description`, commit subjects
`feat(<bead-id>): ...` (`fix`, `docs`, `chore`), the PR title the same subject and the PR body
naming the bead and the originating GitHub issue if one exists. Work in the worktree the fleet view
prepared under `.cerebro/worktrees/`, never in the main checkout. Squash-merge with
`--delete-branch` once the gate and review are green; never `--auto`. Close the bead with
`bd close <id> --reason "Delivered in PR #NN"`, close the parent if that was its last open child,
and `bd dolt push`.

### Committing and Merging to main

`./scripts/gate-full.sh` runs the whole checkpoint below in order and stops at the first failure;
`./scripts/gate-full.sh --fast` runs the fmt, host clippy and unit-test subset for quick iteration.
They are what `.cerebro/project.conf` declares as `gate_full` and `gate_fast`, so a producer runs
exactly this list before opening a pull request. Before merging to main, every item shall pass:

- Run `cargo clippy --all-targets --all-features -- -D warnings` and fix all warnings
- Run `cargo fmt` and fix any formatting issues
- Run `./scripts/test-dir.sh <changed-dirs>` to verify the affected modules pass quickly, e.g.:
  ```bash
  ./scripts/test-dir.sh src/nes/cartridge          # changed cartridge code
  ./scripts/test-dir.sh src/nes --skip-integration # NES changes, fast iteration
  ```
- Run `cargo test --no-default-features --lib` to verify the full test suite passes before creating a PR
- Run `cargo test --doc` and fix all failures. Doctests are compiled only by this command, so a doc example can rot indefinitely while still looking authoritative: six of them had been failing on main until #3175, three importing the nonexistent path `neser::cartridge` and three being ASCII register diagrams rustdoc tried to compile as Rust. Fence prose blocks as ` ```text `; never silence a real example with `ignore`/`no_run` just to make the gate pass
- Run `cargo clippy --target wasm32-unknown-unknown --no-default-features --features wasm --all-targets -- -D warnings` and fix all findings (the host clippy run above cannot see `#[cfg(target_arch = "wasm32")]` code)
- Run `cargo clippy --no-default-features --features frontend --all-targets -- -D warnings` and fix all findings (the `neser` binary requires only `frontend`, so this is a buildable configuration, but neither run above covers it — the host one enables `native`, and the wasm one is a different target. Three warnings sat unnoticed in that gap until #3137)
- Run `wasm-pack test --headless --chrome --no-default-features --features wasm` and fix all warnings and ensure all tests pass
- Run `source .venv/bin/activate && python -m unittest discover -s scripts -t . -p "test_*.py"` and ensure all tests pass
- Run `ruff check scripts` and fix all findings (do not add `noqa` without a stated reason)
- Run `ruff format scripts` and fix any formatting issues
- Run `mypy --config-file scripts/pyproject.toml scripts` and fix all type errors
- Run `npm test` and ensure all tests pass (runs Vitest for web frontend JS unit tests)

Note that it is ok to commit to a feature branch that does not pass all checkpoints, but it is NOT ok to merge to main if any checkpoint fails. Always ensure that all checkpoints pass before merging to main.

## Framework decisions

Where appropriate, use established crates to streamline development and leverage existing solutions. However, ensure that the chosen crates align with the project's requirements and do not introduce unnecessary complexity. Regularly evaluate the suitability of crates as the project evolves. Take all crate decisions in a collaborative way with the navigator.

## Testing strategies

### Running tests by directory

Use `scripts/test-dir.sh` to run only the tests for specific source directories:

```bash
./scripts/test-dir.sh src/nes/cartridge         # Run only cartridge tests (~4100 tests)
./scripts/test-dir.sh src/gb src/platform        # Run gb + platform tests (~220 tests)
./scripts/test-dir.sh src/nes --skip-integration # NES unit tests only, skip slow integration tests
./scripts/test-dir.sh src/nes/cartridge --list   # List matching tests without running them
```

This is especially useful during development to get fast feedback on the area you're working on. The CI workflow uses the same approach to skip irrelevant tests based on changed files.

Integration tests (`nes::integration_tests`, ~740 tests) account for 97% of test execution time. Use `--skip-integration` for fast iteration, then run the full suite before creating a PR.

Testing of the emulator should be done using a mix of unit and integration tests. Unit tests should be used to verify the correctness of individual components and modules, ensuring that each part of the emulator functions as intended in isolation. Integration tests should be employed to validate the interactions between different components, ensuring that they work together seamlessly to provide the desired functionality of the emulator as a whole.

### Unit testing

Unit test should be of both black and white box variety. Black box tests should focus on testing the public interfaces and behaviors of modules without knowledge of their internal workings. They should perferably be tested against the specifications found on https://www.nesdev.org/wiki/. White box tests should be used to test specific internal functions and logic, ensuring that the implementation details are correct. In such cases, the tests should have knowledge of the internal structure of the code being tested and can use internal variables and states to verify correctness.

### Integration testing

Integration tests should cover end-to-end scenarios that validate the overall functionality of the emulator. These tests should simulate real-world usage and interactions, ensuring that all components work together as expected. Integration tests can include running actual NES ROMs and verifying their output against known good results, as well as testing the emulator's performance and stability under various conditions. Integration tests should always be defined against either a well known ROMs behaviour or specifications found on https://www.nesdev.org/wiki/.

### Mapper testing

In roms/automated_tests/mapper_verification/ there is a makefile system to create mapper ROMs. ROMs to be used for verifying mapper implementations. The system should be used to create ROMs that test specific features and behaviors of the mappers, ensuring that they are implemented correctly and function as intended. When creating new ROMs for testing mappers, it is imperative that you NEVER NEVER EVER look at the implementation of the mapper being tested. This ensures that the tests are unbiased and truly validate the functionality of the mapper based on its specifications rather than its implementation details. Specifications for mappers can be found on https://www.nesdev.org/wiki/Mapper and sub pages. Should that not be available, there are fallback specifications on https://nesdev-wiki.nes.science/wikipages/Special_AllPages.xhtml#INES.
When implementing verification ROM for a mapper, ensure to cover all relevant submappers for that specific mapper.

## Communication with user

When asking questions to the user, always try to use the question UI/tool with pre-defined answers. This makes communication more efficient and reduces the risk of misunderstandings. If the question cannot be answered with predefined options there also need to be a free text option to use.

## Repository-specific guidance

- Be sure to always document the configuration opsions in `neser.conf.example`.
- Test ROMs live in `roms/`; keep the existing files and names intact.
- Always keep README.md up to date with major changes to the project, especially if they affect how to run or test the emulator.
- Always keep `architecture.md` up to date when code changes affect the project's module structure, directory layout, binaries, scripts, key design decisions, or testing strategy.
