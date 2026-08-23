CARGO ?= cargo
CARGO_BUILD_JOBS ?= 4
VERSION ?= 0.1.0

.PHONY: all fmt lint check test release-notes-test corpus sources manifests generate generate-check smoke smoke-check toenizer-report package verify doctor clean

all: verify

fmt:
	$(CARGO) fmt --check

lint:
	CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) $(CARGO) clippy --workspace --all-targets --locked -- -D warnings

check:
	CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) $(CARGO) check --workspace --locked

test: release-notes-test
	$(CARGO) toen test

release-notes-test:
	./scripts/tests/release-notes.sh

corpus:
	$(CARGO) run --release --locked --bin toenctl -- corpus check

sources:
	$(CARGO) run --release --locked --bin toenctl -- sources verify --metadata-only

manifests:
	$(CARGO) run --release --locked --bin toenctl -- manifests check

generate:
	$(CARGO) run --release --locked --bin toenctl -- generate

generate-check:
	$(CARGO) run --release --locked --bin toenctl -- generate --check

smoke:
	$(CARGO) run --release --locked --bin toenctl -- bench smoke

smoke-check:
	$(CARGO) run --release --locked --bin toenctl -- bench smoke --check

toenizer-report:
	$(CARGO) toen toenizer report

package:
	$(CARGO) run --release --locked --bin toenctl -- package --version $(VERSION)

verify:
	$(CARGO) toen verify

doctor:
	$(CARGO) toen doctor

clean:
	$(CARGO) clean
