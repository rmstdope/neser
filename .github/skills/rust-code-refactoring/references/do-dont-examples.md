# Rust Refactoring Do/Don't Examples

## Do/Don't (Mixed)

- Do: Consolidate duplicated parsing logic into a shared helper. Don't: Keep copy-pasted parse loops in each module.
- Do: Use consistent names for the same concept. Don't: Mix `mapper_id` and `mapper_number` across modules.

```rust
// Do: Prefer borrowing and iterators when possible.
fn sum(values: &[u32]) -> u32 {
    values.iter().copied().sum()
}

// Don't: Clone just to satisfy the loop shape.
fn sum(values: &[u32]) -> u32 {
    values.to_vec().into_iter().sum()
}
```

```rust
// Do: Model intent with enums for clarity.
enum Mirroring {
    Horizontal,
    Vertical,
}

// Don't: Use bool flags with unclear meaning.
fn set_mirroring(is_horizontal: bool) {
    if is_horizontal { /* ... */ }
}
```

```rust
// Do: Keep error types domain-specific in libraries.
#[derive(thiserror::Error, Debug)]
enum ParseError {
    #[error("invalid header")]
    InvalidHeader,
}

// Don't: Use `String` errors in library code.
fn parse() -> Result<(), String> {
    Err("invalid header".to_string())
}
```

```rust
// Do: Express ownership explicitly.
struct Bus {
    cpu: Cpu,
}

// Don't: Hide ownership behind `Rc<RefCell<T>>` without need.
struct Bus {
    cpu: std::rc::Rc<std::cell::RefCell<Cpu>>,
}
```
