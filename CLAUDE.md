# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

kubempf is a Rust CLI tool for forwarding and maintaining multiple port forwards to Kubernetes pods simultaneously. It's an enhanced version of `kubectl port-forward` that handles multiple services at once with features like service discovery, named port resolution, and automatic reconnection.

## Common Commands

### Development
- `cargo build` - Build the project
- `cargo test` - Run tests  
- `cargo run -- [ARGS]` - Run the tool with arguments
- `cargo fmt` - Format code
- `cargo clippy` - Run linter

### Release Building
- `./build-release.sh` - Cross-compile for Windows and Linux targets using the `cross` tool
- Release binaries are placed in `release/` directory

### Coverage Analysis
- `./cov-report.sh` - Generate LLVM-based code coverage reports
- `./cov-show.sh` - Show coverage analysis

## Architecture

The codebase is organized into focused modules:

- `main.rs` - Entry point with service discovery and async runtime setup
- `cli.rs` - Command-line argument parsing using clap
- `pod.rs` - Core pod selection and port forwarding logic  
- `errors.rs` - Custom error types using thiserror
- `cancelable_stream.rs` - Custom async I/O wrapper for graceful connection cancellation

### Key Design Patterns

- Uses Kubernetes API client (`kube` crate) for service discovery and pod selection
- Async/await with Tokio runtime for concurrent port forwarding
- Structured error handling with `thiserror` and `anyhow`
- Pod selection supports readiness checks, random selection, and namespace scoping
- Graceful connection handling with custom cancelable streams

### Dependencies

- `kube` + `k8s-openapi` for Kubernetes integration
- `tokio` for async runtime
- `clap` for CLI parsing
- `tracing` for structured logging
- `rand` for random pod selection

Dependencies are automatically kept up-to-date using Renovate bot (configured in `renovate.json`).

## Development Workflow

### Planning Changes
Before making any non-trivial changes to the codebase:

1. **Create a comprehensive plan** in `docs/wip/` with the following format:
   - Filename: `YYYY-MM-DD-feature-name.md` or `YYYY-MM-DD-issue-N.md`
   - Include: problem statement, proposed solution, implementation steps, testing approach, potential risks
   - Review the plan before starting implementation

2. **Document decisions** that affect architecture or user experience
3. **Consider backwards compatibility** for CLI arguments and behavior

### Code Quality
- Always run `cargo fmt`, `cargo clippy`, and `cargo test` before commits
- Ensure all tests pass and no clippy warnings exist
- Use structured error handling with custom error types where appropriate

## Testing

Integration tests use `tests/sts-manifest.yaml` StatefulSet for Kubernetes testing scenarios.