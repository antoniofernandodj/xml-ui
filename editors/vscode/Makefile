# Glacier GSS — VS Code extension
#
# Local install only (no Marketplace publishing). Requires Node.js/npx and the
# `code` CLI on PATH (VS Code → Command Palette → "Shell Command: Install 'code'
# command in PATH" if `code` is missing).

NAME    := glacier-gss
VERSION := $(shell node -p "require('./package.json').version")
VSIX    := $(NAME)-$(VERSION).vsix
VSCE    := npx --yes @vscode/vsce

.PHONY: help package install reinstall uninstall clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

package: $(VSIX) ## Build the .vsix package

$(VSIX): package.json language-configuration.json syntaxes/*.json snippets/*.code-snippets
	$(VSCE) package

install: package ## Build and install the extension into VS Code
	code --install-extension $(VSIX) --force

reinstall: uninstall install ## Reinstall (uninstall + fresh build/install)

uninstall: ## Remove the extension from VS Code
	-code --uninstall-extension antoniofernandodj.$(NAME)

clean: ## Delete built .vsix packages
	rm -f *.vsix
