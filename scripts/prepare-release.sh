#!/usr/bin/env bash
#
# prepare-release.sh - Prepare a CC-rDeviceAgent release version bump.
#
# This script creates a release-prep branch, updates this repo's release
# versions, refreshes Cargo.lock, commits the changes, pushes the branch, and
# opens a pull request. After the PR is merged to the default branch, run the
# "Create Release" GitHub workflow with the same version.
#
# Usage:
#   ./scripts/prepare-release.sh <version>
#
# Examples:
#   ./scripts/prepare-release.sh 1.0.0
#   ./scripts/prepare-release.sh 1.0.0-rc1
#   ./scripts/prepare-release.sh 1.0.0-beta.1

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
die() { error "$*"; exit 1; }

usage() {
    echo -e "${BOLD}Usage:${NC} $0 <version>"
    echo ""
    echo -e "${BOLD}Examples:${NC}"
    echo "  $0 1.0.0"
    echo "  $0 1.0.0-rc1"
    echo "  $0 1.0.0-beta.1"
    echo ""
    echo -e "${BOLD}Version format:${NC} MAJOR.MINOR.PATCH[-PRERELEASE]"
}

if [ $# -ne 1 ]; then
    usage
    exit 1
fi

VERSION="$1"
TAG="v$VERSION"
SDK_CRATE="cc-rdeviceagent-app-sdk"

# Keep this in sync with .github/workflows/create-release.yml.
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    usage
    die "Invalid version format: '$VERSION'"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"
APP_SDK_README="$REPO_ROOT/crates/app-sdk/README.md"
PAYLOAD_EXAMPLE_TOML="$REPO_ROOT/examples/payload-hello/Cargo.toml"

cd "$REPO_ROOT"

if [ ! -f "$CARGO_TOML" ]; then
    die "Cargo.toml not found at $CARGO_TOML"
fi

if ! git rev-parse --git-dir >/dev/null 2>&1; then
    die "Not a git repository"
fi

if [ -n "$(git status --porcelain)" ]; then
    warn "Working tree is not clean. Commit or stash local changes before preparing a release."
    git status --short
    exit 1
fi

REMOTE="$(git remote get-url remote 2>/dev/null || true)"
if [ -z "$REMOTE" ]; then
    die "No 'remote' remote configured"
fi

if ! command -v gh >/dev/null 2>&1; then
    die "'gh' CLI not found. Install it from https://cli.github.com/"
fi

if ! gh auth status >/dev/null 2>&1; then
    die "gh is not authenticated. Run: gh auth login"
fi

DEFAULT_BRANCH="$(git remote show remote 2>/dev/null | sed -n '/HEAD branch/s/.*: //p' | head -n1)"
if [ -z "$DEFAULT_BRANCH" ]; then
    for candidate in main master; do
        if git show-ref --verify --quiet "refs/remotes/remote/$candidate"; then
            DEFAULT_BRANCH="$candidate"
            break
        fi
    done
fi
if [ -z "$DEFAULT_BRANCH" ]; then
    die "Could not detect default branch from remote"
fi

info "Default branch: ${BOLD}$DEFAULT_BRANCH${NC}"

if git rev-parse "$TAG" >/dev/null 2>&1; then
    die "Tag '$TAG' already exists locally"
fi
if git ls-remote --tags remote "refs/tags/$TAG" 2>/dev/null | grep -q "refs/tags/$TAG"; then
    die "Tag '$TAG' already exists on remote"
fi

CURRENT_PACKAGE_VERSION="$(awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version = "/ {
        gsub(/"/, "", $3)
        print $3
        exit
    }
' "$CARGO_TOML")"

CURRENT_WORKSPACE_VERSION="$(awk '
    /^\[workspace\.package\]$/ { in_workspace = 1; next }
    /^\[/ { in_workspace = 0 }
    in_workspace && /^version = "/ {
        gsub(/"/, "", $3)
        print $3
        exit
    }
' "$CARGO_TOML")"

if [ -z "$CURRENT_PACKAGE_VERSION" ]; then
    die "Could not read root package version from Cargo.toml"
fi
if [ -z "$CURRENT_WORKSPACE_VERSION" ]; then
    die "Could not read workspace package version from Cargo.toml"
fi
if [ "$CURRENT_PACKAGE_VERSION" != "$CURRENT_WORKSPACE_VERSION" ]; then
    die "Root package version ($CURRENT_PACKAGE_VERSION) and workspace version ($CURRENT_WORKSPACE_VERSION) differ"
fi
if [ "$CURRENT_WORKSPACE_VERSION" = "$VERSION" ]; then
    die "Version is already $VERSION"
fi

BRANCH="chore/release-v$VERSION"

echo ""
echo -e "${BOLD}This will:${NC}"
echo "  1. Create branch: $BRANCH"
echo "  2. Update root package and workspace versions: $CURRENT_WORKSPACE_VERSION -> $VERSION"
echo "  3. Update the payload example package version"
echo "  4. Update crates/app-sdk/README.md dependency example"
echo "  5. Run cargo check --workspace --all-targets"
echo "  6. Commit, push, and open a PR targeting $DEFAULT_BRANCH"
echo ""
echo -e "${BOLD}After PR merge:${NC}"
echo "  Run the Create Release workflow with version=$VERSION."
echo "  It will create tag $TAG, publish $SDK_CRATE to crates.io, and publish the GHCR image."
echo ""
read -rp "Proceed? [y/N] " yn
if [[ ! "$yn" =~ ^[Yy]$ ]]; then
    die "Aborted by user"
fi

info "Fetching remote/$DEFAULT_BRANCH..."
git fetch remote "$DEFAULT_BRANCH"

if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
    warn "Branch '$BRANCH' already exists locally"
    read -rp "Delete and recreate it? [y/N] " yn
    if [[ "$yn" =~ ^[Yy]$ ]]; then
        git branch -D "$BRANCH"
    else
        die "Aborted by user"
    fi
fi

if git show-ref --verify --quiet "refs/remotes/remote/$BRANCH"; then
    warn "Branch '$BRANCH' already exists on remote"
    read -rp "Delete remote/$BRANCH and continue? [y/N] " yn
    if [[ "$yn" =~ ^[Yy]$ ]]; then
        git push remote --delete "$BRANCH"
    else
        die "Aborted by user"
    fi
fi

git checkout -b "$BRANCH" "remote/$DEFAULT_BRANCH"
success "Created branch '$BRANCH'"

CHANGED_FILES=(Cargo.toml Cargo.lock)
if [ -f "$PAYLOAD_EXAMPLE_TOML" ]; then
    CHANGED_FILES+=("examples/payload-hello/Cargo.toml")
fi
if [ -f "$APP_SDK_README" ]; then
    CHANGED_FILES+=("crates/app-sdk/README.md")
fi

cleanup_temp_files=()
cleanup() {
    for file in "${cleanup_temp_files[@]}"; do
        rm -f "$file"
    done
}
trap cleanup EXIT

info "Updating Cargo.toml versions..."
CARGO_TMP="$(mktemp)"
cleanup_temp_files+=("$CARGO_TMP")
awk -v new_version="$VERSION" '
    /^\[package\]$/ {
        section = "package"
        print
        next
    }
    /^\[workspace\.package\]$/ {
        section = "workspace.package"
        print
        next
    }
    /^\[/ {
        section = ""
        print
        next
    }
    section == "package" && /^version = "/ {
        print "version = \"" new_version "\""
        package_updated = 1
        next
    }
    section == "workspace.package" && /^version = "/ {
        print "version = \"" new_version "\""
        workspace_updated = 1
        next
    }
    { print }
    END {
        if (!package_updated || !workspace_updated) {
            exit 42
        }
    }
' "$CARGO_TOML" > "$CARGO_TMP" || die "Failed to update Cargo.toml versions"
mv "$CARGO_TMP" "$CARGO_TOML"
success "Updated Cargo.toml"

if [ -f "$PAYLOAD_EXAMPLE_TOML" ]; then
    info "Updating payload example version..."
    EXAMPLE_TMP="$(mktemp)"
    cleanup_temp_files+=("$EXAMPLE_TMP")
    awk -v new_version="$VERSION" '
        /^\[package\]$/ {
            section = "package"
            print
            next
        }
        /^\[/ {
            section = ""
            print
            next
        }
        section == "package" && /^version = "/ {
            print "version = \"" new_version "\""
            updated = 1
            next
        }
        { print }
        END {
            if (!updated) {
                exit 42
            }
        }
    ' "$PAYLOAD_EXAMPLE_TOML" > "$EXAMPLE_TMP" || die "Failed to update payload example version"
    mv "$EXAMPLE_TMP" "$PAYLOAD_EXAMPLE_TOML"
    success "Updated examples/payload-hello/Cargo.toml"
fi

if [ -f "$APP_SDK_README" ]; then
    info "Updating App SDK README dependency example..."
    perl -0pi -e "s/cc-rdeviceagent-app-sdk = \"[^\"]+\"/cc-rdeviceagent-app-sdk = \"$VERSION\"/g" "$APP_SDK_README"
    success "Updated crates/app-sdk/README.md"
fi

ROOT_VERSION_AFTER="$(awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version = "/ {
        gsub(/"/, "", $3)
        print $3
        exit
    }
' "$CARGO_TOML")"

WORKSPACE_VERSION_AFTER="$(awk '
    /^\[workspace\.package\]$/ { in_workspace = 1; next }
    /^\[/ { in_workspace = 0 }
    in_workspace && /^version = "/ {
        gsub(/"/, "", $3)
        print $3
        exit
    }
' "$CARGO_TOML")"

if [ "$ROOT_VERSION_AFTER" != "$VERSION" ]; then
    die "Root package version update failed: got '$ROOT_VERSION_AFTER'"
fi
if [ "$WORKSPACE_VERSION_AFTER" != "$VERSION" ]; then
    die "Workspace package version update failed: got '$WORKSPACE_VERSION_AFTER'"
fi

info "Refreshing Cargo.lock and checking workspace..."
if ! cargo check --workspace --all-targets; then
    error "cargo check failed. Reverting release-prep file changes."
    git checkout -- "${CHANGED_FILES[@]}" 2>/dev/null || true
    exit 1
fi
success "Workspace check passed"

echo ""
info "Changes to commit:"
git --no-pager diff --stat "${CHANGED_FILES[@]}"
echo ""

read -rp "Commit these changes? [y/N] " yn
if [[ ! "$yn" =~ ^[Yy]$ ]]; then
    warn "Aborted by user. Reverting release-prep file changes."
    git checkout -- "${CHANGED_FILES[@]}" 2>/dev/null || true
    git checkout "$DEFAULT_BRANCH" 2>/dev/null || true
    die "Aborted. Branch '$BRANCH' still exists; delete it manually if needed."
fi

git add "${CHANGED_FILES[@]}"
git commit -m "chore: prepare release $VERSION"
success "Committed release prep changes"

info "Pushing branch '$BRANCH'..."
git push -u remote "$BRANCH"
success "Pushed branch"

info "Creating pull request..."
PR_TITLE="chore: prepare release $VERSION"
PR_BODY="$(cat <<PRBODY
Version bump prepared by \`scripts/prepare-release.sh\`.

## Changes
- Bump root package version: \`$CURRENT_PACKAGE_VERSION\` -> \`$VERSION\`
- Bump workspace package version: \`$CURRENT_WORKSPACE_VERSION\` -> \`$VERSION\`
- Refresh \`Cargo.lock\`
- Update App SDK usage docs

## After Merge
Trigger the **Create Release** workflow from the default branch:

\`\`\`bash
gh workflow run create-release.yml --ref $DEFAULT_BRANCH -f version=$VERSION
\`\`\`

The workflow will:
- Verify \`Cargo.toml\` workspace version matches \`$VERSION\`
- Run pre-release checks
- Verify the \`cc-rdeviceagent\` binary reports \`$VERSION\`
- Create tag \`$TAG\`
- Create a GitHub release
- Dispatch the publish workflow for crates.io and GHCR
PRBODY
)"

PR_OUTPUT="$(gh pr create \
    --title "$PR_TITLE" \
    --body "$PR_BODY" \
    --base "$DEFAULT_BRANCH" \
    --head "$BRANCH" 2>&1)" || {
    error "Failed to create PR:"
    echo "$PR_OUTPUT" >&2
    echo ""
    echo "Create it manually with:"
    echo "  gh pr create --title \"$PR_TITLE\" --base \"$DEFAULT_BRANCH\" --head \"$BRANCH\""
    exit 1
}

PR_URL="$(echo "$PR_OUTPUT" | grep -oE 'https://github.com/[^ ]+/pull/[0-9]+' | head -n1 || true)"
if [ -z "$PR_URL" ]; then
    PR_URL="$PR_OUTPUT"
fi

success "Pull request created: $PR_URL"

echo ""
echo -e "${GREEN}${BOLD}Release preparation complete.${NC}"
echo ""
echo -e "${BOLD}Next steps:${NC}"
echo "  1. Review and merge the PR:"
echo "     $PR_URL"
echo ""
echo "  2. After merge, trigger the release workflow:"
echo "     gh workflow run create-release.yml --ref $DEFAULT_BRANCH -f version=$VERSION"
echo ""
echo "  3. The workflow will create $TAG, publish $SDK_CRATE, and publish the GHCR image."
