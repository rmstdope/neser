# Rust Refactoring Smells Checklist

## Structure and Design
- Module does too many unrelated things.
- Duplicate logic across modules or functions.
- Public API exposes internal types or fields without need.
- Deep or cyclic module dependencies.
- Large files with mixed domain responsibilities.

## Ownership and Borrowing
- Excessive `clone()` or `to_owned()` without justification.
- `Rc<RefCell<T>>` where ownership could be made explicit.
- `Arc<Mutex<T>>` used for single-threaded code paths.
- Lifetimes are overly complex due to broad borrowing.

## Error Handling
- Functions return `Result<T, String>` or `Result<T, Box<dyn Error>>` in libraries.
- Error context is missing or repeated by callers.
- Panics used for recoverable errors.

## API and Type Design
- Public functions take concrete collection types instead of generics or slices.
- `bool` parameters where an enum would clarify intent.
- Repeated parameter groups that suggest a struct.
- Inconsistent naming for the same concept across modules.
- Function or type names that hide side effects or ownership expectations.

## Performance and Correctness
- Unnecessary heap allocation in hot paths.
- `unwrap()` in production paths without invariant documentation.
- Repeated parsing or conversion instead of caching or reusing.

## Testing and Safety
- Missing unit tests around refactored areas.
- Unsafe blocks without a clear safety comment.
- Tests that only cover the happy path.
