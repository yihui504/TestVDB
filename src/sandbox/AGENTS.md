<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# sandbox

## Purpose
Docker 沙箱管理模块，负责测试容器的完整生命周期管理：创建、启动、执行测试脚本、收集结果、销毁。所有测试脚本在隔离的 Docker 容器中运行，确保安全性和可重复性。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | 模块声明 |
| `manager.rs` | Sandbox 管理器：容器创建/启动/停止/销毁、Python 执行环境搭建、Sidecar 容器管理 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- `manager.rs` 是沙箱管理的唯一实现，修改容器行为从这里入手
- 沙箱生命周期：create → start → execute → collect → destroy
- SidecarSpec 用于配置 DB 的辅助容器（如 Milvus 的 etcd/MinIO）
- Python 执行环境：自动安装 pip 包（由 TargetPlugin.pip_packages() 指定）

### Testing Requirements
- 修改沙箱逻辑后需运行 `batch` 命令验证容器正常启动和销毁
- 确保容器清理逻辑不会遗漏资源

### Common Patterns
- Sandbox 结构：封装 Docker 容器 ID、端口映射、网络配置
- 执行模式：Python 脚本写入容器 → `docker exec` 执行 → stdout/stderr 收集
- 清理策略：`--cache-images` 时跳过 Docker 清理

## Dependencies

### Internal
- `infra.rs`（底层 Docker 操作）
- `target/`（获取镜像名、端口、Sidecar 配置）

### External
- tokio（异步进程管理）
