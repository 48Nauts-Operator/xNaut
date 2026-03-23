# XNAUT Testing Guide

## Testing the Rust Backend

### Prerequisites

The Rust backend requires system libraries for GUI integration (Tauri uses WebView). On headless servers, you cannot build the full application, but you can:

1. **Syntax Check** - Verify code compiles (needs system libs)
2. **Unit Tests** - Test individual modules
3. **Integration Tests** - Test complete workflows

### System Dependencies (Ubuntu/Debian)

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libglib2.0-dev \
  libpango1.0-dev \
  libsoup-3.0-dev
```

### Running Tests

#### Unit Tests (no system deps required)
```bash
cd src-tauri
cargo test --lib
```

#### Integration Tests
```bash
cargo test --test '*'
```

#### All Tests
```bash
cargo test
```

### Testing Individual Modules

#### PTY Module
```bash
cargo test --lib pty::tests
```

#### State Module
```bash
cargo test --lib state::tests
```

#### AI Module
```bash
cargo test --lib ai::tests
```

#### Triggers Module
```bash
cargo test --lib triggers::tests
```

### Code Quality Checks

#### Clippy (Linting)
```bash
cargo clippy -- -D warnings
```

#### Format Check
```bash
cargo fmt -- --check
```

#### Auto-format
```bash
cargo fmt
```

### Building

#### Development Build
```bash
cargo build
```

#### Release Build
```bash
cargo build --release
```

#### Full Tauri Application
```bash
cargo tauri build
```

## Testing Frontend Integration

### Manual Testing Workflow

1. **Start Development Server**
```bash
cargo tauri dev
```

2. **Test Terminal Creation**
- Open app
- Click "New Terminal"
- Verify PTY session created
- Check terminal displays correctly

3. **Test Terminal Input**
- Type commands in terminal
- Verify input appears
- Verify command execution
- Check output display

4. **Test Multiple Sessions**
- Create 3+ terminal sessions
- Switch between them
- Verify isolation
- Close individual sessions

5. **Test SSH Connection**
- Enter SSH details
- Connect to remote server
- Verify authentication
- Test remote commands

6. **Test AI Features**
- Send prompt to AI
- Verify response
- Test error analysis
- Test command suggestions

7. **Test Triggers**
- Create error trigger
- Run command that produces error
- Verify notification
- Test trigger enable/disable

8. **Test Session Sharing**
- Create share link
- Copy share code
- Join from another window
- Verify synchronized output

### Automated Frontend Tests

Create test file: `tests/integration_test.js`

```javascript
const { invoke } = require('@tauri-apps/api/core');
const { listen } = require('@tauri-apps/api/event');

async function testTerminalCreation() {
  const sessionId = await invoke('create_terminal_session', {
    config: {
      cols: 80,
      rows: 24
    }
  });

  console.assert(sessionId, 'Session ID should be returned');
  return sessionId;
}

async function testTerminalInput(sessionId) {
  await invoke('write_to_terminal', {
    sessionId,
    data: 'echo "Hello XNAUT"\n'
  });

  // Wait for output
  return new Promise((resolve) => {
    listen(`terminal-output:${sessionId}`, (event) => {
      const output = atob(event.payload.data);
      if (output.includes('Hello XNAUT')) {
        resolve(true);
      }
    });

    setTimeout(() => resolve(false), 5000);
  });
}

async function runTests() {
  console.log('Running integration tests...');

  const sessionId = await testTerminalCreation();
  console.log('✓ Terminal creation test passed');

  const outputReceived = await testTerminalInput(sessionId);
  console.assert(outputReceived, 'Should receive terminal output');
  console.log('✓ Terminal I/O test passed');

  await invoke('close_terminal', { sessionId });
  console.log('✓ Terminal cleanup test passed');

  console.log('\n✓ All tests passed!');
}

runTests().catch(console.error);
```

## Performance Testing

### PTY Performance Test

Test PTY throughput and latency:

```bash
# Generate large output
cd src-tauri
cargo run --example perf_test
```

Create `examples/perf_test.rs`:
```rust
use std::time::Instant;

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // Create 10 concurrent PTY sessions
    let mut sessions = vec![];
    for _ in 0..10 {
        // Session creation code
    }

    let duration = start.elapsed();
    println!("Created 10 sessions in {:?}", duration);
}
```

### Memory Profiling

```bash
# Install heaptrack
sudo apt-get install heaptrack

# Profile application
heaptrack cargo run

# Analyze results
heaptrack_gui heaptrack.*.gz
```

## Debugging

### Enable Debug Logging

```bash
RUST_LOG=debug cargo tauri dev
```

### Specific Module Logging

```bash
RUST_LOG=xnaut::pty=trace cargo tauri dev
```

### Debug PTY Issues

Add to code:
```rust
eprintln!("DEBUG: PTY session {} created", session_id);
```

### GDB Debugging

```bash
cargo build
gdb target/debug/xnaut
```

## Common Issues

### PTY Sessions Not Starting

**Symptom**: `create_terminal_session` returns error

**Solutions**:
- Verify shell path exists: `which bash`
- Check permissions on shell executable
- Verify working directory is accessible

### SSH Connection Failures

**Symptom**: `create_ssh_session` fails

**Solutions**:
- Test SSH manually: `ssh user@host`
- Verify SSH key permissions: `chmod 600 ~/.ssh/id_rsa`
- Check network connectivity: `ping host`

### AI Integration Not Working

**Symptom**: AI responses fail or timeout

**Solutions**:
- Verify API key is set
- Test API manually with curl
- Check network proxy settings
- Verify rate limits not exceeded

### Memory Leaks

**Symptom**: Memory usage grows over time

**Solutions**:
- Run with valgrind: `valgrind --leak-check=full ./target/debug/xnaut`
- Use heaptrack profiler
- Check for unclosed sessions
- Verify trigger cleanup

## Continuous Integration

### GitHub Actions Workflow

Create `.github/workflows/rust.yml`:

```yaml
name: Rust CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v3

    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        override: true

    - name: Install Dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget libssl-dev

    - name: Run Tests
      run: cd src-tauri && cargo test --verbose

    - name: Run Clippy
      run: cd src-tauri && cargo clippy -- -D warnings

    - name: Check Formatting
      run: cd src-tauri && cargo fmt -- --check
```

## Coverage Reports

### Install tarpaulin

```bash
cargo install cargo-tarpaulin
```

### Generate Coverage

```bash
cd src-tauri
cargo tarpaulin --out Html --output-dir coverage
```

### View Coverage

```bash
firefox coverage/index.html
```

## Benchmarking

### Install criterion

Add to `Cargo.toml`:
```toml
[dev-dependencies]
criterion = "0.5"
```

### Create Benchmark

`benches/pty_benchmark.rs`:
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn pty_creation_benchmark(c: &mut Criterion) {
    c.bench_function("create_pty_session", |b| {
        b.iter(|| {
            // PTY creation code
        });
    });
}

criterion_group!(benches, pty_creation_benchmark);
criterion_main!(benches);
```

### Run Benchmarks

```bash
cargo bench
```

## Security Testing

### Audit Dependencies

```bash
cargo install cargo-audit
cargo audit
```

### Check for Unsafe Code

```bash
cargo install cargo-geiger
cargo geiger
```

### Fuzzing

```bash
cargo install cargo-fuzz
cargo fuzz init
cargo fuzz run fuzz_target_1
```

## Documentation Testing

### Test Doc Examples

```bash
cargo test --doc
```

### Generate Documentation

```bash
cargo doc --no-deps --open
```

## Stress Testing

### Session Stress Test

Create 100 concurrent sessions:
```rust
#[tokio::test]
async fn stress_test_sessions() {
    let mut handles = vec![];

    for _ in 0..100 {
        let handle = tokio::spawn(async {
            create_terminal_session(/* ... */).await
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
```

## End-to-End Testing

### Use Playwright or Selenium

Test complete user workflows:
1. Launch app
2. Create terminal
3. Execute commands
4. Verify output
5. Test all features
6. Close cleanly

## Test Coverage Goals

- **Unit Tests**: >80% code coverage
- **Integration Tests**: All API endpoints
- **E2E Tests**: Critical user workflows
- **Performance**: <100ms PTY creation
- **Memory**: <5MB per session
- **Concurrency**: 50+ concurrent sessions

## Next Steps

1. Install system dependencies
2. Run `cargo test` to verify all tests pass
3. Run `cargo clippy` to check for issues
4. Run `cargo fmt` to format code
5. Create integration test suite
6. Set up CI/CD pipeline
7. Add benchmarks for critical paths
8. Document any failing tests or known issues
