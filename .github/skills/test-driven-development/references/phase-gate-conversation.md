# TDD Phase-Gate Conversation Template

Use this as a concrete interaction pattern to enforce mandatory approval gates between phases.

## Example flow

### 1) Red phase completion → ask for Green approval

Assistant:
RED phase complete.
- Added failing tests for toast expiry, max visible, and ordering.
- Confirmed they fail.

Proceed to Green Phase?
Options: Proceed to Green Phase / Hold / Custom response

Navigator:
Proceed to Green Phase

### 2) Green phase completion → ask for Refactor approval

Assistant:
GREEN phase complete.
- Implemented minimal code to satisfy the new failing tests.
- Ran focused tests and they pass.

Proceed to Refactor Phase?
Options: Proceed to Refactor Phase / Hold / Custom response

Navigator:
Proceed to Refactor Phase

### 3) Refactor phase completion → ask for Commit approval

Assistant:
REFACTOR phase complete.
- Improved structure/naming without changing behavior.
- Re-ran relevant tests and they still pass.

Proceed to Commit Phase?
Options: Proceed to Commit Phase / Hold / Custom response

Navigator:
Proceed to Commit Phase

### 4) Commit/PR phase completion → ask for Merge approval

Assistant:
COMMIT phase complete.
- Created commit(s), pushed branch, and opened PR.
- All required checks passed.

Proceed to Merge?
Options: Proceed to Merge / Hold / Custom response

Navigator:
Proceed to Merge

## Guardrails

- If approval is not explicit, do not continue to the next phase.
- If the navigator asks for changes, remain in the current phase and address them before asking again.
- Keep approval prompts short and explicit, and always name the next phase exactly.
