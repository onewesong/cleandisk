#!/usr/bin/env bash

set -euo pipefail

requested_version="${1:-patch}"

case "$requested_version" in
  patch|minor|major) ;;
  *)
    if [[ ! "$requested_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]
    then
      echo "Usage: make release [VERSION=patch|minor|major|x.y.z]" >&2
      exit 2
    fi
    ;;
esac

for command in git npm node cargo gh
do
  if ! command -v "$command" >/dev/null 2>&1
  then
    echo "Required command is not installed: $command" >&2
    exit 1
  fi
done

if [[ "$(git branch --show-current)" != "main" ]]
then
  echo "Releases must be created from the main branch." >&2
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]
then
  echo "The working tree must be clean before releasing." >&2
  exit 1
fi

echo "Checking origin/main..."
git fetch origin main --tags

local_commit=$(git rev-parse HEAD)
remote_commit=$(git rev-parse origin/main)
if [[ "$local_commit" != "$remote_commit" ]]
then
  echo "Local main must exactly match origin/main before releasing." >&2
  exit 1
fi

repository=$(gh repo view --json nameWithOwner --jq .nameWithOwner)

echo "Updating version using npm version ${requested_version}..."
npm version "$requested_version" --no-git-tag-version
node scripts/set-version.mjs
cargo check --manifest-path src-tauri/Cargo.toml

version=$(node --print "require('./package.json').version")
tag="v${version}"

if git rev-parse --verify --quiet "refs/tags/${tag}" >/dev/null || \
  git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1
then
  echo "Tag already exists: ${tag}" >&2
  exit 1
fi

expected_files=(
  package.json
  package-lock.json
  src-tauri/Cargo.toml
  src-tauri/Cargo.lock
  src-tauri/tauri.conf.json
)

while IFS= read -r changed_file
do
  allowed=false
  for expected_file in "${expected_files[@]}"
  do
    if [[ "$changed_file" == "$expected_file" ]]
    then
      allowed=true
      break
    fi
  done
  if [[ "$allowed" != true ]]
  then
    echo "Unexpected file changed during version update: ${changed_file}" >&2
    exit 1
  fi
done < <(git diff --name-only)

echo "Running release checks for ${tag}..."
npm ci
npm test
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml

git diff --check
git add "${expected_files[@]}"
git commit -m "chore: release ${tag}"
release_commit=$(git rev-parse HEAD)
git push origin main

echo "Waiting for GitHub CI on ${release_commit}..."
run_id=""
for _ in {1..30}
do
  run_id=$(gh run list \
    --repo "$repository" \
    --workflow CI \
    --commit "$release_commit" \
    --event push \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId // empty')
  if [[ -n "$run_id" ]]
  then
    break
  fi
  sleep 2
done

if [[ -z "$run_id" ]]
then
  echo "Could not find the GitHub CI run for ${release_commit}; no tag was created." >&2
  exit 1
fi

gh run watch "$run_id" --repo "$repository" --exit-status

git tag -a "$tag" -m "CleanDisk ${tag}"
git push origin "$tag"

echo "Waiting for the GitHub Release workflow..."
release_run_id=""
for _ in {1..30}
do
  release_run_id=$(gh run list \
    --repo "$repository" \
    --workflow Release \
    --commit "$release_commit" \
    --event push \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId // empty')
  if [[ -n "$release_run_id" ]]
  then
    break
  fi
  sleep 2
done

if [[ -z "$release_run_id" ]]
then
  echo "Tag ${tag} was pushed, but its Release workflow could not be found." >&2
  exit 1
fi

gh run watch "$release_run_id" --repo "$repository" --exit-status
release_url=$(gh release view "$tag" --repo "$repository" --json url --jq .url)

echo "Published ${tag}: ${release_url}"
