SHELL := /bin/bash

VERSION ?= patch

.PHONY: start release

start:
	@npm run tauri dev

release:
	@./scripts/release.sh "$(VERSION)"
