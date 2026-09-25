# 惯例入口:所有目标仅转发到 cargo xtask(唯一样板,勿在本文件写实现逻辑)
# 无 make 的环境(如纯净 Windows)等价直达:cargo xtask setup / gate / fmt

.DEFAULT_GOAL := help
.PHONY: help setup gate fmt

help: ## 显示本帮助
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-8s\033[0m %s\n", $$1, $$2}'

setup: ## 一步配置(clone 后):hooks + pinned 工具链
	cargo xtask setup

gate: ## 质量门禁:fmt --check + clippy -D warnings + test
	cargo xtask gate

fmt: ## 格式化全部代码
	cargo xtask fmt
