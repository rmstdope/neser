---
name: rust-code-refactoring
description: Guidance for refactoring Rust code with code smells to find and structural best practices.
---

# Rust Refactoring Skill

## Introduction

Use this skill when refactoring Rust code. It highlights Rust-specific code smells, suggests structural improvements, and provides a systematic workflow for safe, small refactors.

## Instructions

1. **Confirm intent**: Identify the refactoring goal (performance, clarity, safety, modularity) and the target scope.
2. **Scan for smells**: Use the checklist in references to spot Rust-specific issues before changing code.
3. **Prefer small, reversible steps**: Split large refactors into tiny changes that keep behavior intact.
4. **Structure-first refactors**:
   - Keep modules cohesive; split by domain, not by type.
   - Prefer explicit ownership boundaries and minimal sharing.
   - Keep public APIs small and stable; hide internal details.
   - Ensure that refactorings do not increase the compelxity of the code.
5. **Error handling**:
   - Use domain-specific error types; avoid `String` errors in libraries.
   - Prefer `thiserror` for library errors and `anyhow` for application-level contexts.
6. **Performance and correctness**:
   - Avoid needless clones; prefer borrowing and iterators.
   - Watch for `RefCell` or `Rc` usage where ownership can be simplified.
7. **Document intent**: Add small comments only for non-obvious invariants or safety assumptions.
8. **Verify**: Ensure tests cover the refactor and update or add tests if behavior changes.

## Trait-Based Code Refactoring

When working with Rust trait implementations, watch for:

1. **Inconsistent trait method implementations** - Different implementers override/use defaults differently
   - Check if all implementations need the same behavior
   - Consider if defaults should apply uniformly across all implementers
   - Document which methods are "must override" vs "may override"

2. **Constructor consistency across trait implementations**
   - Avoid marking constructors as `#[cfg(test)]` only - makes them unavailable to production code
   - All implementations of the same trait should follow the same constructor pattern
   - If factory pattern is needed, use explicit factory functions rather than conditional compilation

3. **Pattern extraction in trait implementations**
   - When multiple trait implementers have identical or very similar code, extract to helper functions
   - Create intermediate struct types to encapsulate repeated patterns (e.g., bank offset calculation)
   - Use generics to parameterize trait implementations over common patterns

4. **Default trait method override completeness**
   - Document trait contract: which methods must be overridden, which may be overridden
   - If a default implementation may be insufficient for some implementers (e.g., complex internal state),
     mark it as "MUST override if condition X" and add compile-time checks
   - Use compile-time markers or assertion tests to catch missing overrides

## References

- references/smells-checklist.md: Rust code smells and structural checklist.
- references/do-dont-examples.md: Do/don't refactoring examples.

## Examples

- Input: "Refactor a large module with mixed responsibilities."
  Output: "Propose a split by domain, move shared types to a `types` module, and reduce public surface."
- Input: "Improve error handling in a parsing module."
  Output: "Introduce a domain error enum and replace `String` errors with typed variants."
