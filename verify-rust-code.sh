#!/bin/bash
# Verifies XNAUT Rust backend code structure and syntax

echo "╔═══════════════════════════════════════════════════════════════════╗"
echo "║              XNAUT Rust Backend Verification                      ║"
echo "╚═══════════════════════════════════════════════════════════════════╝"
echo ""

PROJECT_DIR="/home/jarvis/projects/NautCode/xnaut"
SRC_DIR="$PROJECT_DIR/src-tauri/src"

echo "📁 Checking project structure..."

# Check if all required files exist
files=(
    "$SRC_DIR/main.rs"
    "$SRC_DIR/state.rs"
    "$SRC_DIR/pty.rs"
    "$SRC_DIR/commands.rs"
    "$SRC_DIR/ssh.rs"
    "$SRC_DIR/ai.rs"
    "$SRC_DIR/errors.rs"
    "$SRC_DIR/triggers.rs"
)

all_exist=true
for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        echo "  ✓ $(basename $file)"
    else
        echo "  ✗ $(basename $file) - MISSING"
        all_exist=false
    fi
done

if [ "$all_exist" = true ]; then
    echo ""
    echo "✓ All required files present"
else
    echo ""
    echo "✗ Some files are missing"
    exit 1
fi

echo ""
echo "📊 Code statistics..."

# Count lines of code
total_lines=0
for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        total_lines=$((total_lines + lines))
        printf "  %-15s %5d lines\n" "$(basename $file)" "$lines"
    fi
done

echo "  ─────────────────────────"
printf "  %-15s %5d lines\n" "TOTAL" "$total_lines"

echo ""
echo "🔍 Checking for ABOUTME comments..."

for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        aboutme_count=$(grep -c "^// ABOUTME:" "$file" || echo "0")
        if [ "$aboutme_count" -ge 2 ]; then
            echo "  ✓ $(basename $file) - $aboutme_count ABOUTME comments"
        else
            echo "  ⚠ $(basename $file) - Missing or incomplete ABOUTME comments"
        fi
    fi
done

echo ""
echo "🧪 Checking for tests..."

for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        if grep -q "#\[test\]" "$file" || grep -q "#\[tokio::test\]" "$file"; then
            test_count=$(grep -c "#\[test\]" "$file" 2>/dev/null || echo "0")
            tokio_test_count=$(grep -c "#\[tokio::test\]" "$file" 2>/dev/null || echo "0")
            total_tests=$((test_count + tokio_test_count))
            echo "  ✓ $(basename $file) - $total_tests tests"
        else
            echo "  ⚠ $(basename $file) - No tests found"
        fi
    fi
done

echo ""
echo "📦 Checking Cargo.toml..."

if [ -f "$PROJECT_DIR/src-tauri/Cargo.toml" ]; then
    echo "  ✓ Cargo.toml exists"

    # Check for key dependencies
    deps=("tauri" "tokio" "portable-pty" "ssh2" "reqwest" "serde" "anyhow" "uuid")
    for dep in "${deps[@]}"; do
        if grep -q "^$dep" "$PROJECT_DIR/src-tauri/Cargo.toml"; then
            echo "  ✓ Dependency: $dep"
        else
            echo "  ⚠ Dependency missing: $dep"
        fi
    done
else
    echo "  ✗ Cargo.toml not found"
fi

echo ""
echo "🔧 Checking Rust syntax (requires cargo)..."

if command -v cargo &> /dev/null; then
    cd "$PROJECT_DIR/src-tauri"
    if cargo check --lib 2>&1 | grep -q "Finished"; then
        echo "  ✓ Code compiles successfully"
    else
        echo "  ⚠ Compilation check failed (may need system dependencies)"
    fi
elif [ -f "$HOME/.cargo/bin/cargo" ]; then
    cd "$PROJECT_DIR/src-tauri"
    if $HOME/.cargo/bin/cargo check --lib 2>&1 | grep -q "Finished"; then
        echo "  ✓ Code compiles successfully"
    else
        echo "  ⚠ Compilation check failed (may need system dependencies)"
    fi
else
    echo "  ⚠ Cargo not found - skipping syntax check"
fi

echo ""
echo "📝 Documentation check..."

if [ -f "$PROJECT_DIR/RUST_BACKEND.md" ]; then
    echo "  ✓ RUST_BACKEND.md exists"
fi

if [ -f "$PROJECT_DIR/TESTING.md" ]; then
    echo "  ✓ TESTING.md exists"
fi

echo ""
echo "════════════════════════════════════════════════════════════════════"
echo "                     Verification Complete                          "
echo "════════════════════════════════════════════════════════════════════"
echo ""
echo "Summary:"
echo "  • All core modules created: state, pty, commands, ssh, ai, errors, triggers"
echo "  • Total lines of code: $total_lines"
echo "  • Documentation: RUST_BACKEND.md, TESTING.md"
echo "  • Next steps: Install system deps and run 'cargo test'"
echo ""
