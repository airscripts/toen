CARGO ?= cargo
CARGO_BUILD_JOBS ?= 4
CONTAINER_ENGINE ?= docker
CONTAINER_IMAGE ?= toen-ci:0.1.0
VERSION ?= 0.1.0

.PHONY: all fmt lint check test corpus sources manifests generate generate-check smoke smoke-check toenizer-report package verify doctor container-build container-verify container-test container-package clean

all: verify

fmt:
	$(CARGO) fmt --check

lint:
	CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) $(CARGO) clippy --workspace --all-targets --locked -- -D warnings

check:
	CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) $(CARGO) check --workspace --locked

test:
	$(CARGO) toen test

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

container-build:
	$(CONTAINER_ENGINE) build --pull --file Containerfile --tag $(CONTAINER_IMAGE) .

container-verify: container-build
	$(CONTAINER_ENGINE) run --rm --name toen-ci-verify $(CONTAINER_IMAGE) make verify

container-test: container-build
	$(CONTAINER_ENGINE) run --rm --name toen-ci-test $(CONTAINER_IMAGE) make test

container-package: container-build
	test -d benchmarks/releases/$(VERSION)
	mkdir -p dist
	$(CONTAINER_ENGINE) run --rm \
		--name toen-ci-package \
		--volume "$(CURDIR)/dist:/workspace/dist" \
		--volume "$(CURDIR)/benchmarks/releases:/workspace/benchmarks/releases:ro" \
		$(CONTAINER_IMAGE) make package VERSION=$(VERSION)

clean:
	$(CARGO) clean
