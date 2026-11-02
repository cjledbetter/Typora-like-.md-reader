#!/usr/bin/env bash
# ==============================================================================
# Master E2E Test Suite Runner for Typora-RustReader
#
# Methodology:
# - Tier 1: Feature Coverage (CLI flags, path resolution, atomic writes, inotify events, TOC extraction, doc stats, asset embedding, packaging/install scripts)
# - Tier 2: Boundary & Corner Cases (empty files, non-UTF-8 bytes, invalid flags, deep paths, symlinks, rapid successive writes, permission errors)
# - Tier 3: Cross-Feature Combinations (pairwise interactions: CLI mode + external edits, untitled buffer + atomic save, etc.)
# - Tier 4: Real-World Scenarios (realistic end-to-end integration workflows)
#
# Target Threshold: >= 403 test cases / test assertions
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/typora_e2e_bins"

mkdir -p "${BUILD_DIR}"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${BOLD}${CYAN}====================================================================${NC}"
echo -e "${BOLD}${CYAN}  TYPORA-RUSTREADER: COMPREHENSIVE 4-TIER E2E TEST SUITE RUNNER     ${NC}"
echo -e "${BOLD}${CYAN}====================================================================${NC}"
echo -e "Project Root: ${PROJECT_ROOT}"
echo -e "Test Binaries: ${BUILD_DIR}\n"

TEST_SUITES=(
    "test_cli.rs"
    "test_io_atomic.rs"
    "test_watcher.rs"
    "test_markdown_ast.rs"
    "test_ipc_bridge.rs"
    "test_assets_embedded.rs"
    "test_packaging.rs"
    "test_scenarios_e2e.rs"
    "test_boundary_stress.rs"
    "test_cross_feature.rs"
    "test_feature_matrix.rs"
)

TOTAL_PASSED=0
TOTAL_FAILED=0
SUITES_PASSED=0
SUITES_FAILED=0

cd "${PROJECT_ROOT}"

for suite in "${TEST_SUITES[@]}"; do
    suite_name="${suite%.rs}"
    bin_path="${BUILD_DIR}/${suite_name}"

    echo -ne "Compiling ${BOLD}${suite}${NC}... "
    if rustc --test "${SCRIPT_DIR}/${suite}" -o "${bin_path}" 2>"${BUILD_DIR}/${suite_name}.build.log"; then
        echo -e "${GREEN}COMPILED${NC}"
    else
        echo -e "${RED}BUILD FAILED${NC}"
        cat "${BUILD_DIR}/${suite_name}.build.log"
        SUITES_FAILED=$((SUITES_FAILED + 1))
        continue
    fi

    echo -ne "Running ${BOLD}${suite_name}${NC}... "
    set +e
    out=$("${bin_path}" --nocapture 2>&1)
    exit_code=$?
    set -e

    if [ ${exit_code} -eq 0 ]; then
        # Parse test results from cargo test output: "test result: ok. X passed; 0 failed; ..."
        passed=$(echo "${out}" | grep -oE '[0-9]+ passed' | awk '{print $1}' || echo "0")
        echo -e "${GREEN}PASSED${NC} (${passed} tests)"
        TOTAL_PASSED=$((TOTAL_PASSED + passed))
        SUITES_PASSED=$((SUITES_PASSED + 1))
    else
        failed=$(echo "${out}" | grep -oE '[0-9]+ failed' | awk '{print $1}' || echo "1")
        echo -e "${RED}FAILED${NC} (${failed} failed)"
        echo "${out}" | grep -E 'FAILED|panicked' || true
        TOTAL_FAILED=$((TOTAL_FAILED + failed))
        SUITES_FAILED=$((SUITES_FAILED + 1))
    fi
done

TOTAL_TESTS=$((TOTAL_PASSED + TOTAL_FAILED))

echo -e "\n${BOLD}${CYAN}====================================================================${NC}"
echo -e "${BOLD}E2E TEST EXECUTION SUMMARY${NC}"
echo -e "${BOLD}${CYAN}====================================================================${NC}"
echo -e "Suites Executed : ${#TEST_SUITES[@]}"
echo -e "Suites Passed   : ${GREEN}${SUITES_PASSED}${NC}"
echo -e "Suites Failed   : ${RED}${SUITES_FAILED}${NC}"
echo -e "Total Tests Run : ${BOLD}${TOTAL_TESTS}${NC}"
echo -e "Total Passed    : ${GREEN}${TOTAL_PASSED}${NC}"
echo -e "Total Failed    : ${RED}${TOTAL_FAILED}${NC}"
echo -e "Coverage Target : 403 test cases (TEST_INFRA.md)"

if [ ${TOTAL_PASSED} -ge 403 ] && [ ${TOTAL_FAILED} -eq 0 ]; then
    echo -e "\n${BOLD}${GREEN}✔ ALL E2E TEST SUITES PASSED! (${TOTAL_PASSED} / >=403 TARGET EXCEEDED)${NC}\n"
    exit 0
else
    if [ ${TOTAL_FAILED} -ne 0 ]; then
        echo -e "\n${BOLD}${RED}✘ TEST FAILURES ENCOUNTERED (${TOTAL_FAILED} tests failed)${NC}\n"
        exit 1
    else
        echo -e "\n${BOLD}${YELLOW}⚠ COVERAGE THRESHOLD NOT REACHED (${TOTAL_PASSED} < 403)${NC}\n"
        exit 2
    fi
fi
