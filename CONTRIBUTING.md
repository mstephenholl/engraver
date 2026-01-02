# Contributing to Engraver

Thank you for your interest in contributing!

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch
4. Make your changes
5. Run tests: `cargo test`
6. Run lints: `cargo clippy -- -D warnings`
7. Submit a pull request

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

1. Update documentation if needed
2. Add tests for new features
3. Ensure CI passes
4. Request review from maintainers
