# ICEcube development tasks. Run `make` (or `make help`) to list them.
# Works with the GNU Make 3.81 that ships with macOS.

SHELL := /bin/sh

BACKEND  := backend
FRONTEND := frontend
CHART    := charts/icecube

# --- Development settings (override on the command line: make dev API_PORT=4100) ---
API_PORT        ?= 4000
CLUSTER_NAME    ?= dev
BASIC_AUTH_USER ?= admin
BASIC_AUTH_PASSWORD ?= secret
CERT_DAYS       ?= 30
RUST_LOG        ?= icecube=debug,tower_http=info

# --- Local k3s cluster (make cluster-up) ---
K3S_NAME  ?= icecube-dev
K3S_IMAGE ?= rancher/k3s:latest
K3S_PORT  ?= 16443
DEV_DIR        := $(CURDIR)/.dev
DEV_KUBECONFIG := $(DEV_DIR)/kubeconfig
DEV_NAMESPACES ?= team-a team-b

IMAGE ?= ghcr.io/sofyan48/icecube:dev

# Set to 1 to let dev/run use a cluster other than the k3s dev cluster.
ALLOW_ANY_CLUSTER ?= 0

# Prefer the throwaway dev cluster when it exists, so development never
# touches a real cluster by accident. `make dev KUBECONFIG=...` still wins.
ifneq ($(wildcard $(DEV_KUBECONFIG)),)
ifneq ($(origin KUBECONFIG),command line)
KUBECONFIG := $(DEV_KUBECONFIG)
endif
endif
ifdef KUBECONFIG
export KUBECONFIG
endif

export CLUSTER_NAME BASIC_AUTH_USER BASIC_AUTH_PASSWORD RUST_LOG
export ICECUBE_LISTEN     := 127.0.0.1:$(API_PORT)
export ICECUBE_CERT_DAYS  := $(CERT_DAYS)
export ICECUBE_STATIC_DIR := $(CURDIR)/$(FRONTEND)/dist
export ICECUBE_API        := http://127.0.0.1:$(API_PORT)

.DEFAULT_GOAL := help

.PHONY: help dev dev-backend dev-frontend check-cluster install run build build-backend \
        build-frontend test lint fmt crd helm-lint docker-build cluster-up cluster-down \
        cluster-status clean

help: ## Show this help
	@echo "Usage: make <target> [VAR=value]"
	@echo
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "Quick start:  make cluster-up && make dev   ->  http://localhost:5173 ($(BASIC_AUTH_USER) / $(BASIC_AUTH_PASSWORD))"

# ---------------------------------------------------------------- development

dev: install ## Run backend + frontend with hot reload (UI on :5173)
	@$(MAKE) --no-print-directory check-cluster
	@echo "==> UI   http://localhost:5173   (sign in: $(BASIC_AUTH_USER) / $(BASIC_AUTH_PASSWORD))"
	@echo "==> API  http://$(ICECUBE_LISTEN)"
	@echo "==> Ctrl+C stops both."
	@$(MAKE) --no-print-directory -j2 dev-backend dev-frontend

dev-backend: ## Run only the backend (restarts on change when watchexec/cargo-watch is installed)
	@cd $(BACKEND) && \
	if command -v watchexec >/dev/null 2>&1; then \
		watchexec --restart --watch src --watch Cargo.toml --exts rs,toml -- cargo run; \
	elif command -v cargo-watch >/dev/null 2>&1; then \
		cargo watch --watch src --watch Cargo.toml -x run; \
	else \
		echo "(tip: install watchexec for auto-restart: brew install watchexec)"; \
		cargo run; \
	fi

dev-frontend: install ## Run only the Vite dev server (proxies /api to the backend)
	@# Wait for the backend (first build + CRD setup) so the proxy does not log errors.
	@i=0; until curl -s -o /dev/null $(ICECUBE_API)/healthz; do \
		i=$$((i+1)); if [ $$i -eq 1 ]; then echo "==> Waiting for the backend on $(ICECUBE_API)..."; fi; \
		if [ $$i -gt 300 ]; then echo "Backend did not start; starting the UI anyway."; break; fi; \
		sleep 1; \
	done
	@cd $(FRONTEND) && bun run dev --host 127.0.0.1 --port 5173 --strictPort

check-cluster: ## Show which cluster the backend will use and refuse non-dev clusters
	@command -v kubectl >/dev/null 2>&1 || { echo "kubectl not found"; exit 1; }
	@ctx=$$(kubectl config current-context 2>/dev/null); \
	server=$$(kubectl config view --minify -o jsonpath='{.clusters[0].cluster.server}' 2>/dev/null); \
	echo "==> Cluster: $${ctx:-<none>} ($${server:-unknown}) via $${KUBECONFIG:-~/.kube/config}"; \
	if ! kubectl get --raw /readyz >/dev/null 2>&1; then \
		echo "Cannot reach the cluster. Start a local one with 'make cluster-up'."; exit 1; \
	fi; \
	if [ "$$KUBECONFIG" != "$(DEV_KUBECONFIG)" ] && [ "$(ALLOW_ANY_CLUSTER)" != "1" ]; then \
		echo "Refusing: this is not the dev cluster. ICEcube would install its CRDs, templates"; \
		echo "and default group there. Either start a throwaway cluster:"; \
		echo "    make cluster-up"; \
		echo "or confirm you really want this cluster:"; \
		echo "    make dev ALLOW_ANY_CLUSTER=1"; \
		exit 1; \
	fi

install: $(FRONTEND)/node_modules/.installed ## Install frontend dependencies

$(FRONTEND)/node_modules/.installed: $(FRONTEND)/package.json $(FRONTEND)/bun.lock
	@cd $(FRONTEND) && bun install
	@touch $@

run: build ## Build everything and serve the built UI from the backend (port API_PORT)
	@$(MAKE) --no-print-directory check-cluster
	@echo "==> http://$(ICECUBE_LISTEN)  (sign in: $(BASIC_AUTH_USER) / $(BASIC_AUTH_PASSWORD))"
	@$(BACKEND)/target/release/icecube

# ------------------------------------------------------------------- building

build: build-frontend build-backend ## Build release backend and frontend

build-backend:
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
	@cd $(BACKEND) && cargo clippy --all-targets -- -D warnings
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

# --------------------------------------------------------- local k3s cluster

cluster-up: ## Start a throwaway k3s cluster in Docker (kubeconfig: .dev/kubeconfig)
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
	@echo "Dev cluster is up. 'make dev' will use it automatically."
	@echo "kubectl:  export KUBECONFIG=$(DEV_KUBECONFIG)"

cluster-down: ## Delete the k3s dev cluster and its kubeconfig
	@docker rm -f $(K3S_NAME) >/dev/null 2>&1 || true
	@rm -f $(DEV_KUBECONFIG)
	@echo "Dev cluster removed."

cluster-status: ## Show the dev cluster state and ICEcube objects in it
	@if [ ! -f $(DEV_KUBECONFIG) ]; then echo "No dev cluster (run 'make cluster-up')."; exit 0; fi
	@KUBECONFIG=$(DEV_KUBECONFIG) kubectl get nodes
	@echo
	@KUBECONFIG=$(DEV_KUBECONFIG) kubectl get icecubeusers,icecubegroups 2>/dev/null || echo "ICEcube CRDs not installed yet."

# ------------------------------------------------------------------- cleanup

clean: ## Remove build output (keeps the dev cluster)
	@cd $(BACKEND) && cargo clean
	@rm -rf $(FRONTEND)/dist
