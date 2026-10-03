# OctoScript 天气助手

一个运行在 OctoSense 中的天气助手应用。用户可以查看天气、切换和搜索城市、保存天气偏好，并通过 OctoSense 宿主的助手服务提问。

> **应用使用 OctoScript 编写。** 主要源码是 [`main.splash`](octoscript-weather-assistant/bundle/main.splash)。

## 为什么开发这个应用

天气应用不应该只给出温度数字，还应该帮人快速决定“今天怎么安排”。这个项目希望把天气、地点和个人关注点放在一个简单的日常流程里：打开应用先看当前城市天气；需要关注别的地方时，点城市名就能切换；遇到出行问题时，可以结合当前城市和天气向 OctoSense 助手提问。

城市切换流程参考了用户提供的手机天气界面：

1. 在首页点击当前城市。
2. 在城市管理页查看已保存的城市并直接切换。
3. 若要添加城市，打开搜索页，先选国内或国际热门城市，也可以输入名称搜索。
4. 选中地点后，应用将它保存到城市列表并更新首页天气。

个性化方向是让用户自己选择怕冷、怕热、关注降雨等项目，也能用“+”添加自己的标签。家庭城市和提醒设置先保存在本机，避免在没有相应宿主服务时声称可以通知家人。

## 页面与功能

| 页面 | 当前内容 |
| --- | --- |
| 今天 | 当前城市、温度、体感、天气状况、今日高低温和基础出行提示；城市入口打开城市管理。 |
| 城市管理 | 展示已保存地点，选择某个城市即可切换。 |
| 城市搜索 | 提供国内外热门城市；支持搜索本地常用地点索引，并通过地理编码服务查找其他名称。 |
| 天空 | 当前城市逐小时预报和未来 7 天预报。 |
| 问答 | 把城市、已有天气摘要和用户问题交给 OctoSense 助手。 |
| 我的 | 保存体感偏好、自定义标签、家庭关注城市和本机提醒开关。 |

### 城市切换

首页城市入口直接转到城市管理；选中城市时更新当前地点、保存列表并刷新天气：

```splash
ButtonFlatter{
    text: current_city.name + " · " + current_city.country + "   ›   切换城市"
    on_click: || goto("cities")
}

fn choose_city(city){
    current_city = city
    let already_saved = false
    for i in cities.len() {
        if cities[i].name == city.name && cities[i].country == city.country {
            already_saved = true
        }
    }
    if !already_saved { cities.push(city) }
    page = "today"
    save_data()
    refresh_weather()
}
```

### 天气搜索

应用先匹配内置常用地点，所以输入“晋”时可以列出晋源区、晋城、晋中等结果；本地索引没有匹配项时，再请求 Open-Meteo 地理编码服务。

```splash
for place in local_places {
    if place.name.search(search_text) >= 0 || place.admin1.search(search_text) >= 0 {
        local_results.push(place)
    }
}
if local_results.len() > 0 {
    city_results = local_results
    ui.content.render()
    return
}
```

### OctoSense 助手接法

天气问答采用 OctoSense 提供的应用会话接口。应用只提交当前地点、已有天气摘要和用户问题，不保存模型服务商凭据：

```splash
host.request("octos.session.open", {}, fn(open_result){
    if !open_result.is_ok {
        chat_messages.push({speaker: "天气助手", text: "助手暂不可用，请检查 OctoSense 授权和 AI 设置。"})
        return
    }
    host.request("octos.turn.start", {text: prompt}, fn(answer){
        if answer.is_ok {
            chat_messages.push({speaker: "天气助手", text: answer.data.text})
        }
    })
})
```

首次使用需要宿主授权，也需要 OctoSense 中已配置可用的 AI。当前本地 `card-host` 预览不提供 `octos` 助手服务，因此已验证服务不可用时的提示，但尚未在真实 OctoSense 宿主中验证 AI 回复。

## 界面截图

以下图片是应用在 card-host 中的实际界面截图，不是设计稿。

<table>
  <tr>
    <td align="center"><strong>已保存城市与切换入口</strong><br><img src="octoscript-weather-assistant/bundle/screenshots/02-cities.png" width="320" alt="城市管理页面"></td>
    <td align="center"><strong>热门城市搜索</strong><br><img src="octoscript-weather-assistant/bundle/screenshots/03-search.png" width="320" alt="热门城市搜索页面"></td>
  </tr>
  <tr>
    <td align="center"><strong>输入“晋”后的地点候选</strong><br><img src="octoscript-weather-assistant/bundle/screenshots/04-search-jin.png" width="320" alt="晋字搜索结果"></td>
    <td align="center"><strong>偏好与自定义标签</strong><br><img src="octoscript-weather-assistant/bundle/screenshots/06-mine.png" width="320" alt="我的页面和自定义标签"></td>
  </tr>
</table>

## 技术结构

```text
octoscript-weather-assistant/
├── BRIEF.md                       # 产品目标与范围
├── PRIVACY.md                     # 数据和隐私说明
├── README.md                      # 应用目录说明
└── bundle/                        # OctoSense 应用包
    ├── manifest.json              # 应用 ID、权限、网络域名和存储额度
    ├── listing.json               # 名称、说明、商店截图和发布信息
    ├── main.splash                # OctoScript 应用逻辑与界面
    ├── assets/icon.svg            # 应用图标
    └── screenshots/               # card-host 实际截图
```

| 部分 | 实现 |
| --- | --- |
| 应用语言 | OctoScript（Splash），由 OctoSense 宿主解释运行。 |
| 天气服务 | Open-Meteo Forecast API。 |
| 城市搜索 | 内置常用地点索引；其他输入走 Open-Meteo Geocoding API。 |
| 本地数据 | OctoSense 应用私有存储，保存城市、偏好、家庭城市和提醒开关。 |
| AI 问答 | `octos.session.open` 与 `octos.turn.start`，由宿主处理授权和 AI 配置。 |
| 运行权限 | `storage`、`net`、`octos.session.open`、`octos.turn.start`；网络只声明天气和地理编码服务域名。 |

## 开发与验证

应用包通过 OctoSense App Hub 的本地 `hub check` 检查，使用了 unsigned 开发模式。城市列表、搜索输入与结果、自定义标签和本地保存流程已在 Windows card-host 中交互验证，并检查过应用重启后的保存数据。

在当前受限运行环境里，天气服务网络请求未能完成；请在有网络的 OctoSense 宿主中刷新并确认天气数据。AI 回复也需要在支持 `octos` 的真实宿主中授权后验证。

本仓库保存应用源代码和 bundle，不包含 Rust 天气应用。开发时需要另外准备官方 OctoScript-App-Design-Flow 与 OctoSense App Hub 工具链；具体安装和宿主要求见 [官方开发流程](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) 与 [App Hub 文档](https://github.com/OctoSense-org/OctoSense-App-Hub)。准备好工具链后可运行：

```powershell
python <OctoScript-App-Design-Flow 路径>\tools\octo check octoscript-weather-assistant\bundle
python <OctoScript-App-Design-Flow 路径>\tools\octo run octoscript-weather-assistant\bundle --port 8141
python <OctoScript-App-Design-Flow 路径>\tools\octo shot 8141 preview.png
```

## 当前边界

- 天气偏好和自定义标签可保存、查看与移除；首页天气建议目前是基础版，尚未把每种用户偏好都接入独立的建议规则。
- 家庭城市与“提醒开关”只记录在应用本机；当前版本不会发送 Matrix 消息，也不会创建系统通知。
- 城市搜索优先使用应用内置的常用地点索引；索引之外的地点依赖地理编码服务和网络。
- 问答需要 OctoSense 宿主提供助手服务，并由用户在宿主授权和配置 AI。
- 尚未完成 App Hub 正式签名和提交。

## 数据和隐私

应用把当前城市、保存城市、天气偏好、家庭关注城市和提醒开关写入 OctoSense 为应用分配的私有目录。查询非内置地点时会将搜索词发送给 Open-Meteo 地理编码服务。使用问答时，城市、已有天气摘要和问题通过宿主助手服务处理。详细说明见 [`PRIVACY.md`](octoscript-weather-assistant/PRIVACY.md)。

## 许可证

仓库使用 Apache-2.0 许可证，详见 [`LICENSE`](LICENSE)。
