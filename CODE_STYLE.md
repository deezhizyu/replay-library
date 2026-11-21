# Code style

## No duplication

- Search the codebase for an existing implementation before writing a feature, helper, or utility.
- If logic is needed in a second place, extract it and reuse it. Do not copy and adjust.
- One concept, one name, one place.

## Readable, idiomatic Rust

- Follow Rust conventions: `snake_case` for functions and variables, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for constants, standard module layout, `Result` and `?` for errors, iterators over index loops.
- No shortened names. Write `reminder_interval`, not `rem_int`. Write `configuration`, not `cfg`. Write `index`, not `i`. Short names are acceptable only for conventional generics (`T`) and closure parameters whose meaning is obvious from a one-line closure.
- Code must pass `cargo fmt` and `cargo clippy -- -D warnings`.

## No comments

- The code speaks for itself. If it does not, rename, extract a function, or restructure until it does.
- Add a comment only for something truly quirky: a platform workaround, a non-obvious invariant, a deliberate oddity that would otherwise look like a bug. Say why, in one line.
- Do not leave commented-out code, TODOs, or section banners.

## Keep it simple

- About 90% of code can be written very simply. Write it that way. Do not overcomplicate simple things.
- No abstraction, configuration option, trait, generic, or builder that has no second user today.
- Prefer plain functions and plain data over layers of indirection.

## Compute once

- If a value is used more than once, compute it once, store it in a variable, and reuse it.
- Do not repeat the same call, lookup, or conversion in a loop or in neighboring lines.

## Simple, then optimized

- The code should be simple, and as a result optimized. Pick the simple data structure and algorithm that does not waste work (no needless clones, allocations, or repeated passes).
- Do not trade readability for micro-optimizations. Optimize only with a measured reason.

## No code debt from the start

- Write each piece as the final version, not a placeholder to clean up later.
- Handle errors properly the first time. No `unwrap` or `expect` on fallible runtime input, and no silently ignored errors.
- Add a test with the code for any non-trivial logic. Keep pure logic separate from UI and platform code so it is testable.
- Fix the root cause. Do not patch symptoms.

## Dependencies

- Add a dependency only after vetting it as described in `AGENTS.md`. Prefer the standard library and crates already in the tree.

## Commit messages

- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `build:`, `ci:`, with an optional scope, for example `feat(overlay): fade in and out`.
- Subject in the imperative mood, at most 72 characters, no trailing period.
- One logical change per commit.
