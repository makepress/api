_default:
    @just --list

# Runs clippy on the source
check:
    cargo clippy --locked -- -D warnings

# Runs unit tests
test:
    cargo test --locked