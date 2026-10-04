# 在途 MiniMax 后台

Python 3.11 及以上，无需安装依赖。密钥只存在于后台进程环境，不写入应用包。

在仓库的 `onway/` 目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File backend/start-ai.ps1
```

按提示输入自己的 MiniMax API Key。该窗口运行后台，Ctrl+C 停止。仓库启动脚本默认读取 `.local-state` 开发数据根目录中的 `onway/` 子目录。连接已安装应用时，通过 `-AppData <宿主应用数据根目录>` 指定宿主使用的目录。必须与宿主 `--app-data` 相同，不能指向 bundle。在应用原有设置中找到“智能通勤”，阅读数据说明并开启开关；建议会直接显示在通勤卡片内。

开发预览在另一个终端启动：

```powershell
python <OctoScript-App-Design-Flow路径>/tools/octo run bundle --port 8143 --app-data .local-state --detach
```

默认模型 `MiniMax-M2.5`，国内地址 `https://api.minimax.cn/v1`，遵循 [MiniMax 官方 OpenAI 兼容接口](https://platform.minimax.cn/docs/api-reference/text-openai-api)。可在启动前设置 `MINIMAX_MODEL`；国际账号可设置 `MINIMAX_BASE_URL=https://api.minimax.io/v1`。使用开放平台 API Key，后台不会调用 Coding Plan 的 Anthropic 接口。

## API

服务只监听 `127.0.0.1:8787`，不直接暴露公网。HTTP API 与文件桥共用分析器。应用通过私有文件桥连接，不尝试绕过 OctoSense 的 HTTPS 网络限制，不需要后台访问令牌。

| 接口 | 用途 |
| --- | --- |
| `GET /health` | 是否启动、是否配置密钥；不调用模型 |
| `POST /v1/ai/analyze` | 分析输入，生成并保存建议卡片 |
| `GET /v1/cards/latest` | 读取最近结果 |

后两个接口需要 `Authorization: Bearer <ONWAY_API_TOKEN>`；启动前在后台环境设置令牌。未设置时 HTTP 数据接口拒绝访问，文件桥仍可运行。不启用跨域访问。

分析请求示例（`epoch` 要替换成当前 Unix 秒数）：

```json
{
  "schema": 1,
  "epoch": 1791000000,
  "enabled": true,
  "demo": true,
  "stage": "idle",
  "locationTrusted": false,
  "config": {
    "homeName": "家",
    "workName": "公司",
    "selectedPlan": 0,
    "threshold": 10,
    "anomalyNotice": true
  },
  "history": [{"durMin": 47, "plan": 0}]
}
```

结果包含 `action: push_card | none`、标题、正文、理由、卡片 ID、阶段、预览标记、过期时间和状态。模型只允许生成建议，不能修改行程或路线。无密钥、网络异常、无效 JSON 和非法动作都有可见错误提示。不会把 MiniMax 上游响应或个人上下文写入日志。

## 后台与推送范围

应用每3秒写 `ai-context.json`，后台读取并分析，原子写入 `ai-result.json`；应用每3秒读取结果。提交地点名称、方案、阈值、是否有可信定位和最近30条保存历史；不提交精确坐标或整份应用目录。开关默认关闭，关闭后停止提交给模型。

同一上下文10分钟内不重复调用；模型调用至少间隔60秒；卡片至少间隔5分钟，同文案30分钟内去重。后台保存去重账本，重启保留限频。数据超过90秒拒绝分析，卡片10分钟过期，应用只接收阶段及预览标记匹配的卡片。卡片不会打断地点编辑或设置。

这是**应用内卡片推送**。宿主关闭后不采集新位置，也不提供系统通知、锁屏或跨设备推送；后台仍可运行，但拒绝过期快照。路线、班次目前只有原型示例，AI 不能提供已验证的实时到站或延误。将来接入真实路线数据或系统卡片分发，需要对应数据源和宿主能力。

## 验证

```powershell
python -m unittest discover -s backend -p 'test_*.py' -v
node integrate-ai.cjs
```

`sync-prototypes.cjs` 已调用同一集成器，重新生成原型不会丢失 AI 功能。新 AI 界面使用 Noto Sans SC 字体子集，覆盖基本汉字区（U+4E00–U+9FFF）、拉丁字母和常用标点；生僻扩展汉字和 emoji 不在子集范围。许可证沿用 `bundle/assets` 中的 SIL OFL。

接口、解析、文件桥、开关、过期、去重及上游失败通过本地测试。真实 MiniMax 回复需配置有效密钥后验证，本地测试不代表已验证线上模型质量。
