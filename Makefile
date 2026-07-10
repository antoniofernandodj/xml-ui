# glacier-ui — tarefas de topo.
#
# Instala as extensões de VS Code a partir da raiz do projeto. Cada extensão
# tem seu próprio Makefile em editors/; aqui só delegamos.

GV  := editors/vscode-gv
GSS := editors/vscode

.PHONY: help install-gv install-gss install-extensions reinstall-extensions uninstall-extensions

help: ## Lista os alvos
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'

install-gv: ## Instala a extensão Glacier View (.gv) no VS Code
	$(MAKE) -C $(GV) install

install-gss: ## Instala a extensão Glacier GSS (.gss) no VS Code
	$(MAKE) -C $(GSS) install

install-extensions: install-gv install-gss ## Instala as duas extensões de VS Code

reinstall-extensions: ## Reempacota e reinstala as duas extensões
	$(MAKE) -C $(GV) reinstall
	$(MAKE) -C $(GSS) reinstall

uninstall-extensions: ## Remove as duas extensões do VS Code
	$(MAKE) -C $(GV) uninstall
	$(MAKE) -C $(GSS) uninstall
