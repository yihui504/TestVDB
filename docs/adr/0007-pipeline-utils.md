# _pipeline_utils：共享脚本基础设施

**Status**: Accepted (2026-07-01)

## Context

`scripts/` 下 11 个脚本各自独立实现了相同的基础设施：

- **JSON 安全读取**：`safe_read(path)` 在 `check_cache.py` 和 `novelty_gate.py` 中各有一份独立实现
- **路径解析**：`os.path.join(session_dir, "debate_logs", "stage2_aggregation.json")` 出现在 5+ 个文件中
- **Windows UTF-8 编码**：`sys.stdout.reconfigure(encoding="utf-8")` 出现在 `check_cache.py` 和 `novelty_gate.py` 的 `if sys.platform == "win32"` 块中
- **日志文件查找**：`_find_log(sd, script_name)` 在 `verify_live_l1.py` 中，其他脚本各自用 glob 实现
- **CLI 参数解析**：`argparse` 和 `sys.argv` 风格不一致（有的用位置参数，有的用 `--flag`）

每个脚本单独看都能用，但作为**集合**，它们缺少共享模块。每个脚本的接口就是它的 CLI——庞大且不一致。

## Decision

创建 `scripts/_pipeline_utils.py` 作为共享基础设施模块。下划线前缀惯例表示"内部模块，非公共 API"。

### 接口

```python
# —— JSON I/O ——
def read_json(path: str | Path) -> dict | None
    """安全读取 JSON。文件不存在或解析失败返回 None。"""

def write_json(path: str | Path, data: Any) -> bool
    """安全写入 JSON。自动创建父目录。返回 True 表示成功。"""

# —— 路径 ——
def debate_log_path(session_dir: str | Path, name: str) -> Path
    """session_dir / debate_logs / {name}.json"""

def session_path(session_dir: str | Path, *parts: str) -> Path
    """session_dir / parts..."""

def plugin_root() -> Path | None
    """环境变量优先，否则从脚本位置推断。"""

# —— 日志与执行 ——
def find_log(session_dir: str | Path, script_name: str) -> Path | None
    """在 session_dir 下查找脚本对应的 output_*.log 文件。"""

# —— 编码 ——
def setup_encoding():
    """Windows: sys.stdout/stderr reconfigure UTF-8。幂等。"""
```

### 使用示例

```python
# 改造前
import json, os
path = os.path.join(session_dir, "debate_logs", "stage2_aggregation.json")
try:
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    data = None

# 改造后
from _pipeline_utils import read_json, debate_log_path
data = read_json(debate_log_path(session_dir, "stage2_aggregation"))
```

### 消费者迁移

每个脚本约减少 15 行重复样板代码。不影响脚本的外部接口（CLI 参数不变）。

## Consequences

- **局部性**：路径约定变更 = 改 1 个文件。Windows 编码设置 = 1 处 `setup_encoding()` 调用。
- **杠杆**：11 个脚本从一个 import 获得安全 I/O + 一致错误处理 + 干净路径。
- **可测试性**：JSON I/O 模式一次测试（`read_json` 缺文件/损坏 JSON/权限错误）。脚本测试中 mock 这些函数。
- **删除测试**：删掉此模块会让 11 份相同样板代码回归——它在挣口粮。
- **风险**：如果 `_pipeline_utils.py` 变得太大（超过 400 行），按关注点拆分为 `_pipeline_io.py` + `_pipeline_paths.py`。当前预估 < 150 行。
