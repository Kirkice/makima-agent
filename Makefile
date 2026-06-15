.PHONY: help dev up down build logs clean test lint format

# ── 默认目标 ─────────────────────────────────
help:  ## 显示帮助信息
	@echo "Makima Agent Backend"
	@echo ""
	@echo "可用命令:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

# ── Docker Compose ──────────────────────────
up:  ## 启动所有服务（后台运行）
	docker compose up -d

down:  ## 停止所有服务
	docker compose down

build:  ## 重新构建所有镜像
	docker compose build

logs:  ## 查看服务日志
	docker compose logs -f

restart:  ## 重启所有服务
	docker compose restart

# ── 基础设施 ────────────────────────────────
infra:  ## 仅启动基础设施（PostgreSQL + Redis）
	docker compose up -d postgres redis

infra-down:  ## 停止基础设施
	docker compose stop postgres redis

# ── 开发 ────────────────────────────────────
dev:  ## 启动开发环境（基础设施 + 热重载）
	docker compose up -d postgres redis
	@echo "基础设施已启动，请使用以下连接信息:"
	@echo "  PostgreSQL: localhost:5432 (makima/makima)"
	@echo "  Redis:      localhost:6379"

install:  ## 安装 Python 依赖
	cd apps/backend && pip install -e ".[dev]"

# ── 代码质量 ────────────────────────────────
lint:  ## 运行代码检查
	cd apps/backend && ruff check src/
	cd apps/backend && mypy src/

format:  ## 格式化代码
	cd apps/backend && ruff format src/

test:  ## 运行测试
	cd apps/backend && pytest tests/ -v

# ── 清理 ────────────────────────────────────
clean:  ## 清理构建产物
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "*.egg-info" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true

clean-all: clean  ## 清理所有（包括 Docker 数据）
	docker compose down -v
	docker system prune -f