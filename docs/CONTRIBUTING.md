# Contributing to Engraver

Thank you for your interest in contributing!

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch
4. Make your changes
5. Run tests: `cargo test`
6. Run linting: `cargo clippy -- -D warnings`
7. Submit a pull request

## Pre-Commit Hooks

This project uses [cargo-husky](https://github.com/rhysd/cargo-husky) for pre-commit hooks.
Hooks are **automatically installed** when you run `cargo build` or `cargo test`.

The pre-commit hook runs:
- `cargo fmt --check` - Verify code formatting
- `cargo clippy --all-targets --all-features -- -D warnings` - Lint checking
- `cargo test --lib` - Unit tests

To bypass hooks temporarily (not recommended):
```bash
git commit --no-verify
```

## Code Style

- Follow Rust conventions
- Run `cargo fmt` before committing
- Address all clippy warnings
- Add tests for new functionality
- Document public APIs

## Safety

This project deals with raw disk I/O. Safety is paramount:

- Never skip safety checks
- Test on virtual machines first
- Document any unsafe code thoroughly
- Consider edge cases carefully

## Pull Request Process

1. Update documentation as needed
2. Add unit, integration, and e2e tests for new features
3. Ensure CI passes
4. Request review from maintainers
