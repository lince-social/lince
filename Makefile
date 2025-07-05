procs:
	# cargo install mprocs --locked
	mprocs \
    "bacon . --job clippy-all" \
    "dx serve --platform desktop" \
    "systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
	"systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat"

.PHONY: dev

dev:
	@MDBOOK_BIN=$$(which mdbook 2>/dev/null || echo "$$HOME/.cargo/bin/mdbook"); \
	if [ ! -x "$$MDBOOK_BIN" ]; then \
		echo "mdbook not found. Installing..."; \
		cargo install mdbook; \
	fi; \
	echo "Using mdbook at $$MDBOOK_BIN"; \
	"$$MDBOOK_BIN" serve --port 6173

build:
	mdbook build;
	git add .
	git commit
	git push
.PHONY: dev build install

MDBOOK_BIN := $(shell which mdbook 2>/dev/null || echo $(HOME)/.cargo/bin/mdbook)

install:
	@if [ ! -x "$(MDBOOK_BIN)" ]; then \
		echo "mdbook not found. Installing..."; \
		cargo install mdbook; \
	else \
		echo "Using mdbook at $(MDBOOK_BIN)"; \
	fi

dev: install
	@$(MDBOOK_BIN) serve --port 9999

build: install
	@$(MDBOOK_BIN) build -d ./dist
