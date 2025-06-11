# Unit Tests Enhancement Plan - Issue #5

**Date**: 2025-06-11  
**Issue**: [#5 - Unit Tests](https://github.com/narthollis/kubempf/issues/5)  
**Milestone**: 1.0.0  
**Priority**: High  

## Problem Statement

The kubempf project currently has minimal test coverage despite being a critical infrastructure tool. Issue #5 specifically calls for "unit tests for stuff like the arg parsing and port selection." Current analysis shows:

- Only 7 tests exist, all focused on CLI forward specification parsing
- No tests for core functionality like pod selection, port forwarding, or Kubernetes integration
- No integration tests or error injection testing
- Critical gaps in argument parsing edge cases and validation

## Current Test Coverage Analysis

### Existing Tests (cli.rs only)
✅ Basic forward specification parsing formats  
✅ IPv4/IPv6 address parsing  
✅ Namespace specification parsing  
✅ One error case (string port without local port)  

### Critical Gaps
❌ CLI argument validation and flag parsing  
❌ Pod selection and readiness filtering logic  
❌ Port resolution and mapping  
❌ Service discovery and API interaction  
❌ Error handling and edge cases  
❌ Kubernetes configuration loading  
❌ Connection forwarding mechanics  

## Proposed Solution

### Phase 1: Core Unit Tests (Week 1)
**Focus**: Address issue #5 directly with comprehensive unit tests for argument parsing and port selection

1. **Enhanced CLI Testing** (`src/cli.rs`)
   - Complete CliArgs struct validation
   - Command-line flag combinations testing
   - Multiple forwards argument parsing
   - Edge cases: invalid addresses, port ranges, malformed input
   - Error message quality validation

2. **Port Selection Testing** (`src/pod.rs`)
   - `find_pod_port()` function unit tests
   - Named vs numeric port resolution
   - Service port mapping logic
   - Error cases: missing ports, invalid port names

3. **Service Resolution Testing** (`src/main.rs`)
   - Service lookup and validation
   - Port resolution between services and pods
   - Configuration loading edge cases

### Phase 2: Core Logic Testing (Week 2)
**Focus**: Test critical business logic components

1. **Pod Selection Logic** (`src/pod.rs`)
   - `find_pod()` function testing with mock data
   - Readiness filtering scenarios
   - Random selection algorithm validation
   - Empty result handling

2. **Error Handling** (`src/errors.rs`)
   - Error type construction and formatting
   - Error message clarity and helpfulness
   - Error propagation chains

3. **Cancelable Stream Testing** (`src/cancelable_stream.rs`)
   - AsyncRead/AsyncWrite implementations
   - Abort signal handling
   - Connection reset error concealment

### Phase 3: Integration Testing (Week 3)
**Focus**: End-to-end functionality validation

1. **Mock Kubernetes Integration**
   - Mock kube-client for service/pod APIs
   - Service discovery scenarios
   - Pod readiness state changes
   - Network policy constraints

2. **Connection Forwarding**
   - TCP socket binding and forwarding
   - IPv4/IPv6 dual stack support
   - Connection lifecycle management
   - Graceful shutdown handling

## Implementation Steps

### Step 1: Test Infrastructure Setup
```bash
# 1. Create test module structure
mkdir -p src/tests
touch src/tests/mod.rs
touch src/tests/cli_tests.rs
touch src/tests/pod_tests.rs
touch src/tests/integration_tests.rs

# 2. Add test dependencies to Cargo.toml
# - tokio-test for async testing
# - mockall for mocking
# - proptest for property-based testing
# - tempfile for temporary test files
```

### Step 2: CLI Testing Enhancement
```rust
// Comprehensive CliArgs testing
// Multiple forward specifications
// Flag combination validation
// Error message quality checks
```

### Step 3: Port Selection Testing
```rust
// find_pod_port() unit tests
// Service port mapping scenarios
// Named port resolution
// Error case handling
```

### Step 4: Mock Infrastructure
```rust
// Mock Kubernetes API responses
// Test data fixtures
// Async test utilities
```

## Testing Approach

### Test Categories
1. **Unit Tests**: Individual function testing with mocked dependencies
2. **Integration Tests**: Component interaction testing
3. **Property Tests**: Input validation with generated test cases
4. **Error Injection**: Failure scenario testing

### Test Data Strategy
- Create reusable test fixtures for Kubernetes objects
- Use property-based testing for argument parsing validation
- Mock Kubernetes API responses for deterministic testing
- Generate edge case scenarios programmatically

### Coverage Goals
- **Phase 1**: 60% coverage focusing on CLI and port selection
- **Phase 2**: 75% coverage including core business logic
- **Phase 3**: 85% coverage with integration testing

## Potential Risks

### Technical Risks
1. **Async Testing Complexity**: Tokio async testing can be tricky
   - **Mitigation**: Use tokio-test crate and established patterns

2. **Kubernetes API Mocking**: Complex API surface to mock
   - **Mitigation**: Focus on essential API calls, use mockall traits

3. **Network Testing**: TCP forwarding tests may be flaky
   - **Mitigation**: Use localhost loopback, deterministic port allocation

### Process Risks
1. **Test Maintenance Burden**: Large test suite requires maintenance
   - **Mitigation**: Prioritize high-value tests, use test utilities

2. **Build Time Impact**: Extensive testing may slow CI/CD
   - **Mitigation**: Parallelize tests, optimize test data

## Dependencies

### New Test Dependencies
```toml
[dev-dependencies]
tokio-test = "0.4"      # Async testing utilities
mockall = "0.12"        # Mocking framework  
proptest = "1.4"        # Property-based testing
tempfile = "3.8"        # Temporary file management
wiremock = "0.6"        # HTTP API mocking
```

### Test Infrastructure
- CI/CD integration for test execution
- Coverage reporting setup
- Test result artifacts

## Success Criteria

### Phase 1 Success
- [ ] All CLI argument parsing scenarios tested
- [ ] Port selection logic fully tested
- [ ] Edge cases and error handling covered
- [ ] Test coverage >60%

### Overall Success
- [ ] Issue #5 requirements fully addressed
- [ ] Comprehensive test suite with >85% coverage
- [ ] CI/CD integration working
- [ ] Documentation for test patterns
- [ ] Foundation for future test development

## Timeline

- **Week 1**: Phase 1 - Core unit tests for CLI and port selection
- **Week 2**: Phase 2 - Business logic testing
- **Week 3**: Phase 3 - Integration testing and polish
- **Week 4**: Documentation and CI/CD integration

## Notes

This plan directly addresses issue #5's call for "unit tests for stuff like the arg parsing and port selection" while building a comprehensive testing foundation for the 1.0.0 milestone. The phased approach ensures immediate value delivery while building toward long-term maintainability.

The focus on CLI testing and port selection in Phase 1 will provide immediate confidence in the user-facing interface and core functionality, which are critical for a command-line tool that users depend on for development workflows.