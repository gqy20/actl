# 人类惯例入口:按使用频率排序,仅转发到 cargo xtask(勿在本文件写实现逻辑)
# 已精简为高频目标(2026-10-03):fmt/regress 等低频任务直达 cargo xtask,不再设别名。
# agent 与无 make 的环境一律直达 `cargo xtask …`(完全等价);完整任务清单:`cargo xtask`(无参)。

.DEFAULT_GOAL := help
.PHONY: help dev gate evidence setup

help: ## 显示本帮助
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-8s\033[0m %s\n", $$1, $$2}'

dev: ## 诊断循环(开发期最高频):增量 DEBUG 构建,秒级;勿在诊断里 --release(docs/70 §8)
	cargo xtask dev

gate: ## 质量门禁(提交前必过):fmt --check + clippy -D warnings + test
	cargo xtask gate

evidence: ## 证据轮(每轮验收一次):RELEASE 构建 + 二进制指纹落 target/evidence/
	cargo xtask evidence

setup: ## 一步配置(clone 后一次):hooks + pinned 工具链
	cargo xtask setup
