#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────
# E2E + UI/UX test for expman-rs TensorBoard integration
# ─────────────────────────────────────────────────────
# Screenshots are saved to /tmp first (chrome-devtools-mcp
# restricts file writes to OS temp dir by default), then
# copied to $RESULTS_DIR.

WORKSPACE_DIR="/home/lokesh/Projects/personal/expman-rs"
TEST_DIR="${WORKSPACE_DIR}/test_experiments"
RESULTS_DIR="${WORKSPACE_DIR}/test_results"
TMP_SCREENSHOTS="/tmp/expman_e2e_screenshots"
CHROME_PROFILE="${WORKSPACE_DIR}/test_chrome_profile"
SERVER_PORT=8090
CHROME_PORT=9222
PASSED=0
FAILED=0
TESTS_RUN=0

# Ensure .venv/bin is on PATH so server can invoke tensorboard binary
export PATH="${WORKSPACE_DIR}/.venv/bin:$PATH"

# Verify we're inside nix develop (cargo must be available)
if ! command -v cargo &>/dev/null; then
    echo "ERROR: cargo not found. Run this inside 'nix develop':"
    echo "  nix develop --command bash scratch/run_e2e_test.sh"
    exit 1
fi

# Helpers
pass() { echo "  ✅ PASS: $1"; PASSED=$((PASSED + 1)); TESTS_RUN=$((TESTS_RUN + 1)); }
fail() { echo "  ❌ FAIL: $1"; FAILED=$((FAILED + 1)); TESTS_RUN=$((TESTS_RUN + 1)); }

screenshot() {
    local name="$1"
    local tmp_path="${TMP_SCREENSHOTS}/${name}"
    local final_path="${RESULTS_DIR}/${name}"
    npx -y chrome-devtools-axi screenshot "$tmp_path" 2>/dev/null || true
    if [ -f "$tmp_path" ]; then
        cp "$tmp_path" "$final_path"
        echo "  📸 Screenshot saved: $final_path ($(du -h "$final_path" | cut -f1))"
    else
        echo "  ⚠️  Screenshot not captured: $name"
    fi
}

# ─── Cleanup ──────────────────────────────────────────
echo "═══ Phase 0: Cleanup ═══"
rm -rf "$RESULTS_DIR"
rm -rf "$TMP_SCREENSHOTS"
rm -rf "$CHROME_PROFILE" || true
mkdir -p "$RESULTS_DIR"
mkdir -p "$TMP_SCREENSHOTS"
mkdir -p "$CHROME_PROFILE"

# Kill any lingering server or chrome on our ports
lsof -ti:${SERVER_PORT} 2>/dev/null | xargs kill -9 2>/dev/null || true
lsof -ti:${CHROME_PORT} 2>/dev/null | xargs kill -9 2>/dev/null || true

# Ensure tensorboard python package is installed in venv
if ! command -v tensorboard &>/dev/null; then
    echo "Installing tensorboard in .venv..."
    uv pip install tensorboard tensorboardX protobuf
fi

# ─── Phase 1: Generate test data ─────────────────────
echo ""
echo "═══ Phase 1: Generate test data ═══"

if [ ! -d "$TEST_DIR/e2e_tb_experiment" ] || [ -z "$(find "$TEST_DIR" -name '*.tfevents*' 2>/dev/null)" ]; then
    echo "Generating fresh experiment data..."
    rm -rf "$TEST_DIR"
    cd "$WORKSPACE_DIR"
    uv run --with tensorboardX,protobuf "${WORKSPACE_DIR}/scratch/test_e2e_tensorboard.py"
else
    echo "Reusing existing test data in $TEST_DIR"
fi

# Verify data structure
RUN_DIR=$(find "$TEST_DIR/e2e_tb_experiment" -mindepth 1 -maxdepth 1 -type d | head -n 1)
if [ -z "$RUN_DIR" ]; then
    echo "FATAL: No run directory found"
    exit 1
fi
RUN_NAME=$(basename "$RUN_DIR")
echo "  Run directory: $RUN_DIR"
echo "  Run name: $RUN_NAME"

# Test: experiment structure
if [ -d "$RUN_DIR/tensorboard" ]; then
    pass "TensorBoard log directory exists"
else
    fail "TensorBoard log directory missing"
fi

TFEVENTS=$(find "$RUN_DIR/tensorboard" -name "*.tfevents*" 2>/dev/null | head -1)
if [ -n "$TFEVENTS" ]; then
    pass "tfevents file exists: $(basename "$TFEVENTS")"
else
    fail "No tfevents file found"
fi

# Check for expman metrics
if find "$RUN_DIR" -name "*.parquet" -o -name "metrics.*" 2>/dev/null | grep -q .; then
    pass "expman metrics file exists"
else
    fail "expman metrics file missing"
fi

# ─── Phase 2: Build & start server & Chrome ───────────────────
echo ""
echo "═══ Phase 2: Build & start expman server & Headless Chrome ═══"
cd "$WORKSPACE_DIR"

export EXPMAN_SKIP_FRONTEND_BUILD=1

echo "Building expman (cli+server features)..."
cargo build --features cli,server 2>&1 | tail -3

echo "Starting expman server on port $SERVER_PORT..."
cargo run --features cli,server -- serve -p "$SERVER_PORT" "$TEST_DIR" &
SERVER_PID=$!

echo "Starting Headless Chrome on port $CHROME_PORT..."
/usr/bin/google-chrome-stable --headless --remote-debugging-port=$CHROME_PORT --disable-gpu --user-data-dir="$CHROME_PROFILE" --no-sandbox &
CHROME_PID=$!

cleanup() {
    echo ""
    echo "═══ Cleanup ═══"
    kill "$SERVER_PID" 2>/dev/null || true
    kill "$CHROME_PID" 2>/dev/null || true
    npx -y chrome-devtools-axi stop 2>/dev/null || true
    rm -rf "$TMP_SCREENSHOTS" 2>/dev/null || true
    rm -rf "$CHROME_PROFILE" 2>/dev/null || true
}
trap cleanup EXIT

# Wait for server and Chrome
echo "Waiting for server and Chrome to start..."
for i in {1..20}; do
    if curl -sf "http://localhost:${SERVER_PORT}/api/experiments" >/dev/null 2>&1 && curl -sf "http://localhost:${CHROME_PORT}/json/version" >/dev/null 2>&1; then
        echo "  Server & Chrome are up!"
        pass "Server & Chrome started successfully"
        break
    fi
    if [ "$i" -eq 20 ]; then
        fail "Server or Chrome failed to start within 20s"
        echo "FATAL: Cannot proceed without server and Chrome"
        exit 1
    fi
    sleep 1
done

# ─── Phase 3: API tests ──────────────────────────────
echo ""
echo "═══ Phase 3: API tests ═══"

EXPERIMENTS=$(curl -sf "http://localhost:${SERVER_PORT}/api/experiments" 2>/dev/null || echo "")
if echo "$EXPERIMENTS" | grep -q "e2e_tb_experiment"; then
    pass "GET /api/experiments returns e2e_tb_experiment"
else
    fail "GET /api/experiments does not list the experiment"
fi

RUNS=$(curl -sf "http://localhost:${SERVER_PORT}/api/experiments/e2e_tb_experiment/runs" 2>/dev/null || echo "")
if echo "$RUNS" | grep -q "$RUN_NAME"; then
    pass "GET .../runs returns the run ($RUN_NAME)"
else
    fail "GET .../runs does not list the run"
fi

RUN_DETAIL=$(curl -sf "http://localhost:${SERVER_PORT}/api/experiments/e2e_tb_experiment/runs/$RUN_NAME" 2>/dev/null || echo "")
if [ -n "$RUN_DETAIL" ]; then
    pass "GET .../runs/$RUN_NAME returns run details"
else
    fail "GET .../runs/$RUN_NAME returns empty"
fi

# ─── Phase 4: Browser UI tests ───────────────────────
echo ""
echo "═══ Phase 4: Browser UI tests ═══"

export CHROME_DEVTOOLS_AXI_SESSION="expman-e2e-$$"
export CHROME_DEVTOOLS_AXI_BROWSER_URL="http://127.0.0.1:${CHROME_PORT}"

echo "Opening main dashboard..."
OPEN_RESULT=$(npx -y chrome-devtools-axi open "http://localhost:${SERVER_PORT}" 2>&1 || true)
sleep 3

if echo "$OPEN_RESULT" | grep -qi "RootWebArea\|expman\|experiment\|page"; then
    pass "Main dashboard page loads"
else
    fail "Main dashboard page did not load"
fi

screenshot "01_main_dashboard.png"

# Navigate to experiment page
echo "Navigating to experiment page..."
npx -y chrome-devtools-axi open "http://localhost:${SERVER_PORT}/experiments/e2e_tb_experiment" 2>/dev/null || true
sleep 3

SNAPSHOT=$(npx -y chrome-devtools-axi snapshot 2>&1 || true)
screenshot "02_experiment_page.png"

if echo "$SNAPSHOT" | grep -qi "$RUN_NAME\|run\|metric\|tensorboard\|all"; then
    pass "Experiment page shows run content"
else
    fail "Experiment page missing expected content"
fi

# Select all runs to reveal details
echo "Selecting all runs..."
npx -y chrome-devtools-axi eval "
  (function() {
    const buttons = Array.from(document.querySelectorAll('button'));
    const allBtn = buttons.find(el => el.textContent.trim().toLowerCase() === 'all');
    if (allBtn) { allBtn.click(); return 'Clicked All'; }
    return 'All button not found';
  })()
" 2>/dev/null || true
sleep 2

screenshot "03_all_runs_selected.png"

# Check for TensorBoard tab
echo "Looking for TensorBoard tab..."
TB_SEARCH=$(npx -y chrome-devtools-axi eval "
  (function() {
    const all = Array.from(document.querySelectorAll('*'));
    const matches = all.filter(el => {
      const text = (el.textContent || '').trim().toLowerCase();
      return (text === 'tensorboard') && el.children.length === 0;
    });
    return matches.length > 0 ? 'Found ' + matches.length : 'Not found';
  })()
" 2>&1 || echo "eval failed")

if echo "$TB_SEARCH" | grep -qi "Found"; then
    pass "TensorBoard tab/section exists in UI"

    npx -y chrome-devtools-axi eval "
      (function() {
        const all = Array.from(document.querySelectorAll('*'));
        const tbEl = all.find(el => {
          const text = (el.textContent || '').trim().toLowerCase();
          return (text === 'tensorboard') && el.children.length === 0;
        });
        if (tbEl) { tbEl.click(); return 'Clicked TensorBoard tab'; }
        return 'Not clicked';
      })()
    " 2>/dev/null || true
    sleep 3

    screenshot "04_tensorboard_tab.png"

    # Click the "All TB Plugins & Profiler" subtab or Launch button
    echo "Switching to Full Engine subtab / launching engine..."
    SWITCH_RESULT=$(npx -y chrome-devtools-axi eval "
      (function() {
        const buttons = Array.from(document.querySelectorAll('button'));
        const fullTab = buttons.find(el => el.textContent.includes('All TB Plugins') || el.textContent.includes('Full'));
        if (fullTab) { fullTab.click(); return 'Clicked Full Engine Subtab'; }
        return 'Subtab not found';
      })()
    " 2>&1 || echo "eval failed")
    echo "  Switch result: $SWITCH_RESULT"
    sleep 2

    # Launch Engine
    LAUNCH_SEARCH=$(npx -y chrome-devtools-axi eval "
      (function() {
        const buttons = Array.from(document.querySelectorAll('button'));
        const launchBtn = buttons.find(el => el.textContent.includes('Launch'));
        return launchBtn ? 'Found: ' + launchBtn.textContent.trim() : 'Not found';
      })()
    " 2>&1 || echo "eval failed")
    echo "  Launch search: $LAUNCH_SEARCH"

    if echo "$LAUNCH_SEARCH" | grep -qi "Found"; then
        pass "Launch TensorBoard button exists in dual UI"

        npx -y chrome-devtools-axi eval "
          (function() {
            const buttons = Array.from(document.querySelectorAll('button'));
            const launchBtn = buttons.find(el => el.textContent.includes('Launch'));
            if (launchBtn) { launchBtn.click(); return 'Clicked Launch Engine'; }
            return 'Not found';
          })()
        " 2>/dev/null || true

        echo "  Waiting for TensorBoard engine to initialize (10s)..."
        sleep 10

        screenshot "05_tensorboard_live.png"

        IFRAME_CHECK=$(npx -y chrome-devtools-axi eval "
          (function() {
            const iframes = document.querySelectorAll('iframe');
            return iframes.length > 0 ? 'Iframes: ' + iframes.length : 'No iframes';
          })()
        " 2>&1 || echo "eval failed")

        if echo "$IFRAME_CHECK" | grep -qE "Iframes: [1-9]"; then
            pass "TensorBoard iframe is present"
        else
            fail "TensorBoard iframe not found"
        fi
    else
        fail "Launch TensorBoard button not found"
    fi
else
    fail "TensorBoard tab/section not found in UI"
fi

screenshot "06_final_state.png"

npx -y chrome-devtools-axi stop 2>/dev/null || true

# ─── Phase 5: Summary ────────────────────────────────
echo ""
echo "═══════════════════════════════════════════"
echo "  E2E Test Summary"
echo "═══════════════════════════════════════════"
echo "  Tests run: $TESTS_RUN"
echo "  Passed:    $PASSED"
echo "  Failed:    $FAILED"
echo ""
echo "  Screenshots in: $RESULTS_DIR"
ls -la "$RESULTS_DIR" 2>/dev/null || true
echo "═══════════════════════════════════════════"

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
