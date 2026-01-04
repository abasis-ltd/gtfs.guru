# Contributing to GTFS Guru

First off, thanks for taking the time to contribute! ❤️

All types of contributions are encouraged and valued. See the [Table of Contents](#table-of-contents) for different ways to help and details about how this project handles them. Please make sure to read the relevant section before making your contribution. It will make it a lot easier for us maintainers and smooth out the experience for all involved. The community looks forward to your contributions.

## Table of Contents

- [I Have a Question](#i-have-a-question)
- [I Want To Contribute](#i-want-to-contribute)
  - [Reporting Bugs](#reporting-bugs)
  - [Suggesting Enhancements](#suggesting-enhancements)
  - [Your First Code Contribution](#your-first-code-contribution)
- [Development Workflow](#development-workflow)
  - [Prerequisites](#prerequisites)
  - [Running Tests](#running-tests)
  - [Style Guide](#style-guide)

## I Have a Question

> If you want to ask a question, we assume that you have read the available [Documentation](https://gtfs.guru).

Before you ask a question, it is best to search for existing [Issues](https://github.com/abasis-ltd/gtfs.guru/issues) that might help you. In case you've found a suitable issue and still need clarification, you can write your question in this issue. It is also advisable to search the internet for answers first.

If you then still feel the need to ask a question and need clarification, we recommend the following:

- Open an [Issue](https://github.com/abasis-ltd/gtfs.guru/issues/new).
- Provide as much context as you can about what you're running into.
- Provide project and platform versions (OS, Rust version, etc), depending on what seems relevant.

## I Want To Contribute

### Reporting Bugs

Specifying verification steps in bug reports ensures that we can fix the bug and verifies that the fix actually works.

**Before Submitting a Bug Report**

1. Check that your issue does not already exist in the [issue tracker](https://github.com/abasis-ltd/gtfs.guru/issues).
2. Check if the issue is reproducible with the latest version of the code.

**How to Submit a Good Bug Report**

- Use the **Bug Report** issue template.
- **Use a clear and descriptive title** for the issue to identify the problem.
- **Describe the exact steps which reproduce the problem** in as many details as possible.
- **Describe the behavior you observed after following the steps** and point out what exactly is the problem with that behavior.
- **Explain which behavior you expected to see instead and why.**
- **Include screenshots and animated GIFs** which show you following the steps and clearly demonstrate the problem.

### Suggesting Enhancements

This section guides you through submitting an enhancement suggestion for GTFS Guru, **including completely new features and minor improvements to existing functionality**. Following these guidelines will help maintainers and the community to understand your suggestion and find related suggestions.

- Use the **Feature Request** issue template.
- **Use a clear and descriptive title** for the issue to identify the suggestion.
- **Provide a step-by-step description of the suggested enhancement** in as many details as possible.
- **Explain why this enhancement would be useful** to most GTFS Guru users.

### Your First Code Contribution

Unsure where to begin contributing to GTFS Guru? You can start by looking through these `good first issue` and `help wanted` issues:

- **Good first issues** - issues which should only require a few lines of code, and a test or two.
- **Help wanted issues** - issues which should be a bit more involved than `good first issue`.

## Development Workflow

### Prerequisites

You will need [Rust](https://www.rust-lang.org/tools/install) installed. We recommend using `rustup` to manage your Rust installation.

### Building parts of the project

The project is a workspace with multiple crates.

- Core logic: `crates/gtfs_validator_core`
- CLI tool: `crates/gtfs_validator_cli`
- Web view: `crates/gtfs_validator_web`

To build the entire workspace:

```bash
cargo build
```

### Running Tests

We take testing seriously. Please ensure all tests pass before submitting a PR.

Run all tests:

```bash
cargo test
```

### Style Guide

We use standard Rust tooling to maintain code quality.

1. **Formatting**: Ensure your code is formatted correctly.

   ```bash
   cargo fmt
   ```

2. **Linting**: Ensure there are no warnings or errors from Clippy.

   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

## Commit Messages

- Use the present tense ("Add feature" not "Added feature")
- Use the imperative mood ("Move cursor to..." not "Moves cursor to...")
- Limit the first line to 72 characters or less
- Reference issues and pull requests liberally after the first line
