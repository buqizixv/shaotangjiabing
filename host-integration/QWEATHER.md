# 和风地区搜索配置

地区搜索使用和风 GeoAPI，天气预报仍由 Open-Meteo 提供。代码只允许城市与区县查询 `GET /geo/v2/city/lookup`、景点查询 `GET /geo/v2/poi/lookup`（固定 `type=scenic`）。一次页面搜索最多各调用一次；家庭城市匹配只查城市。没有自动重试、后台搜索或其他和风接口。内存缓存保留 24 小时，最多 256 个关键词，重启后清空。

凭据在和风控制台仅启用 GeoAPI，控制台数据权限关闭。GeoAPI 属于“天气和基础服务”，目前每账户每月同组 API 合计前 50,000 次免费，之后计费；额度与同账户其他项目共享，不是每个接口各 50,000 次。见[官方价格](https://dev.qweather.com/docs/finance/pricing/)。

宿主默认读取用户目录 `.config/octosense/qweather/config.json`：

```json
{
  "api_host": "你的专属域名.qweatherapi.com",
  "api_key": "你的 API Key"
}
```

API Host 只填域名。可用 `jwt` 替代 `api_key`，只能配置一种。可用 `QWEATHER_CONFIG` 指定文件，或启动前设置 `QWEATHER_API_HOST` 加 `QWEATHER_API_KEY` / `QWEATHER_JWT`。真实配置不能放进仓库、bundle、截图或日志；Windows 私有配置目录限制为当前用户和 SYSTEM 可读写。

本机已创建 Daycast 项目并保存私有配置，该文件不随仓库分发；其他设备的宿主需另行配置。API Key 从 2027 年起将受请求量限制，长期部署可切换 JWT，见[官方认证](https://dev.qweather.com/docs/configuration/authentication/)。

UI 调用 `storage.daycast.places.search`，传入 `query` 和 `scenic`。宿主只接受 Daycast 系统/商店应用 ID，不接受自定义 URL；密钥只用于宿主请求头。结果按 Location ID 去重，同名地点保留省市信息。新版 `no-such-location` HTTP 400 视为空结果，认证、额度与权限错误单独报告。旧请求不能覆盖新搜索。

验证：GeoAPI 单元测试、Daycast 页面/行为回归、太原、榆次、忻州和五台山真实 GeoAPI 响应已执行。手机与商店安装流程尚未验证。
