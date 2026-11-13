# ICEcube development tasks. Run `make` (or `make help`) to list them.
# Works with the GNU Make 3.81 that ships with macOS.
#
# `make dev` is the same as running, in two terminals:
#   cd backend && BASIC_AUTH_PASSWORD=secret cargo run
#   cd frontend && bun install && bun dev
# It uses your current kubeconfig (KUBECONFIG or ~/.kube/config), like those commands.
#
# Backend settings are the normal env vars and can be passed on the command line,
# e.g. `make dev CLUSTER_NAME=prod ICECUBE_CERT_DAYS=7`. See docs/development.md "Configuration".

SHELL := /bin/sh

BACKEND  := backend
FRONTEND := frontend
CHART    := charts/icecube

BASIC_AUTH_PASSWORD ?= secret
export BASIC_AUTH_PASSWORD

# Where the backend listens by default; used to wait for it before starting Vite.
API_URL ?= http://127.0.0.1:4000

# --- Optional local k3s cluster (make dev-local) ---
K3S_NAME  ?= icecube-dev
K3S_IMAGE ?= rancher/k3s:latest
K3S_PORT  ?= 16443
DEV_DIR        := $(CURDIR)/.dev
DEV_KUBECONFIG := $(DEV_DIR)/kubeconfig
DEV_NAMESPACES ?= team-a team-b

IMAGE ?= ghcr.io/sofyan48/icecube:dev

.DEFAULT_GOAL := help

.PHONY: help dev dev-backend dev-frontend dev-local install run build build-backend \
        build-frontend test lint fmt crd helm-lint docker-build cluster-up cluster-down \
        cluster-status clean

help: ## Show this help
	@echo "Usage: make <target> [VAR=value]"
	@echo
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "Quick start:  make dev   ->  http://localhost:5173 (admin / $(BASIC_AUTH_PASSWORD))"

# ---------------------------------------------------------------- development

dev: install ## Backend + frontend against your current kubeconfig (UI on :5173)
	@ctx=$$(kubectl config current-context 2>/dev/null); \
	echo "==> Cluster: $${ctx:-<none>} via $${KUBECONFIG:-~/.kube/config}"
	@echo "==> UI http://localhost:5173  (admin / $(BASIC_AUTH_PASSWORD)).  Ctrl+C stops both."
	@$(MAKE) --no-print-directory -j2 dev-backend dev-frontend

dev-backend: ## Backend only: cd backend && cargo run
	@cd $(BACKEND) && cargo run

dev-frontend: install ## Frontend only: cd frontend && bun dev
	@# Wait for the backend so the Vite proxy does not log errors while it compiles.
	@i=0; until curl -s -o /dev/null $(API_URL)/healthz; do \
		i=$$((i+1)); if [ $$i -eq 1 ]; then echo "==> Waiting for the backend on $(API_URL)..."; fi; \
		if [ $$i -gt 300 ]; then echo "Backend not up yet; starting the UI anyway."; break; fi; \
		sleep 1; \
	done
	@cd $(FRONTEND) && bun dev

dev-local: ## Same as dev, but against a throwaway k3s cluster in Docker
	@$(MAKE) --no-print-directory cluster-up
	@$(MAKE) --no-print-directory dev KUBECONFIG=$(DEV_KUBECONFIG)

install: $(FRONTEND)/node_modules/.installed ## Install frontend dependencies (bun install)

$(FRONTEND)/node_modules/.installed: $(FRONTEND)/package.json $(FRONTEND)/bun.lock
	@cd $(FRONTEND) && bun install
	@touch $@

run: build ## Build the single release binary (UI embedded) and run it on :4000
	@echo "==> http://localhost:4000  (admin / $(BASIC_AUTH_PASSWORD))"
	@$(BACKEND)/target/release/icecube

# ------------------------------------------------------------------- building

build: build-frontend build-backend ## Build one release binary with the UI embedded
	@ls -lh $(BACKEND)/target/release/icecube | awk '{print "==> " $$9 " (" $$5 ", UI embedded)"}'

# The UI must be built first: it is embedded into the binary at compile time.
build-backend: build-frontend
	@cd $(BACKEND) && cargo build --release

build-frontend: install
	@cd $(FRONTEND) && bun run build

docker-build: ## Build the container image (tag IMAGE)
	docker build -t $(IMAGE) .

# ------------------------------------------------------------- quality checks

test: ## Run backend tests
	@cd $(BACKEND) && cargo test

lint: install ## Format check, clippy, type check, chart lint and CRD drift check
	@cd $(BACKEND) && cargo fmt --check
	@cd $(BACKEND) && ICECUBE_ALLOW_NO_UI=1 cargo clippy --all-targets -- -D warnings
	@cd $(FRONTEND) && bunx vue-tsc -b
	@cd $(BACKEND) && cargo run --quiet -- crd | diff -u ../$(CHART)/crds/icecube-crds.yaml - \
		|| { echo "CRDs are out of date: run 'make crd'"; exit 1; }
	@$(MAKE) --no-print-directory helm-lint

fmt: ## Format the backend code
	@cd $(BACKEND) && cargo fmt

crd: ## Regenerate the chart CRDs from backend/src/model.rs
	@cd $(BACKEND) && cargo run --quiet -- crd > ../$(CHART)/crds/icecube-crds.yaml
	@echo "Updated $(CHART)/crds/icecube-crds.yaml"

helm-lint: ## Lint the Helm chart (uses Docker when helm is not installed)
	@if command -v helm >/dev/null 2>&1; then \
		helm lint $(CHART) --strict --set controlPlaneAddress=https://k8s.example.com:6443; \
	else \
		docker run --rm -v $(CURDIR)/charts:/charts alpine/helm:latest \
			lint /charts/icecube --strict --set controlPlaneAddress=https://k8s.example.com:6443; \
	fi

# ------------------------------------------------- optional local k3s cluster

cluster-up: ## Start (or resume) the k3s dev cluster; kubeconfig in .dev/kubeconfig
	@mkdir -p $(DEV_DIR)
	@if docker ps -a --format '{{.Names}}' | grep -qx '$(K3S_NAME)'; then \
		docker start $(K3S_NAME) >/dev/null; \
	else \
		docker run -d --name $(K3S_NAME) --privileged -p $(K3S_PORT):6443 \
			$(K3S_IMAGE) server --tls-san 127.0.0.1 >/dev/null; \
	fi
	@printf "Waiting for k3s"; i=0; \
	until docker exec $(K3S_NAME) kubectl get --raw /readyz >/dev/null 2>&1; do \
		i=$$((i+1)); if [ $$i -gt 120 ]; then echo " timed out"; exit 1; fi; \
		printf "."; sleep 1; \
	done; echo " ready"
	@docker exec $(K3S_NAME) cat /etc/rancher/k3s/k3s.yaml \
		| sed 's#127.0.0.1:6443#127.0.0.1:$(K3S_PORT)#' > $(DEV_KUBECONFIG)
	@for ns in $(DEV_NAMESPACES); do \
		KUBECONFIG=$(DEV_KUBECONFIG) kubectl create namespace $$ns >/dev/null 2>&1 || true; \
	done

cluster-down: ## Delete the k3s dev cluster
	@docker rm -f $(K3S_NAME) >/dev/null 2>&1 || true
	@rm -f $(DEV_KUBECONFIG)
	@echo "Dev cluster removed."

cluster-status: ## Show the k3s dev cluster and ICEcube objects in it
	@if [ ! -f $(DEV_KUBECONFIG) ]; then echo "No dev cluster (make dev-local starts one)."; exit 0; fi
	@KUBECONFIG=$(DEV_KUBECONFIG) kubectl get nodes
	@echo
	@KUBECONFIG=$(DEV_KUBECONFIG) kubectl get icecubeusers,icecubegroups 2>/dev/null || echo "ICEcube CRDs not installed yet."

# ------------------------------------------------------------------- cleanup

clean: ## Remove build output
	@cd $(BACKEND) && cargo clean
	@rm -rf $(FRONTEND)/dist
