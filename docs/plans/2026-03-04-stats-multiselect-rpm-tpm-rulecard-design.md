# 统计与规则卡片增强设计方案

**Date**: 2026-03-04  
**Status**: Draft（待口径确认）

## 1. 背景与目标

围绕日志统计与服务页规则卡片，完成三项增强：

1. 统计页规则筛选从“单选”升级为“多选”。
2. 统计汇总新增 `RPM` 与 `TPM` 指标。
3. 服务页规则卡片新增请求数与 Token 数的轻量趋势图。

目标：在不破坏现有日志采集和聚合架构的前提下，提升运营可观测性与规则级别诊断效率。

## 2. 当前实现基线

- 统计页当前是单个 `ruleFilter` 字符串状态，调用 `refreshLogsStats(hours, ruleKey)`。
- IPC/命令层当前仅支持 `ruleKey?: string`。
- 后端统计聚合 `StatsStore::summarize(hours, rule_key)` 只支持单规则过滤。
- 服务页规则卡片当前只展示规则名、协议、启用状态、额度徽章，无请求/Token 趋势。

## 3. 方案总览

### 3.1 信息架构

- 日志页（Stats Tab）
  - 筛选区：规则多选 + 时间窗口。
  - 请求指标区：请求总数、错误数、成功率、RPM。
  - Token 指标区：输入、输出、缓存命中、缓存写入、Input TPM、Output TPM。
  - 趋势图区：保留现有双轴图（Token 柱 + 请求线）。
  - 速率趋势区：新增 `RPM/TPM` 趋势图。

- 服务页（Rule Card）
  - 每张规则卡新增“轻量统计区”：
    - 请求趋势 Sparkline（24h）
    - Token 趋势 Sparkline（24h）
    - 当前窗口聚合值（请求总数 / Token 总数）

### 3.2 交互原则

- 默认保持“全部规则”，0 学习成本。
- 多选通过“可搜索下拉 + 勾选 + Chips”实现，降低规则多时定位成本。
- 卡片趋势图只做快速感知，不堆复杂图例与控制项。

## 4. 统计口径（需对齐）

### 4.1 时间窗口

- 沿用现有窗口选项：`1h/6h/24h/7d/30d/90d`。
- 统计口径统一基于所选窗口的聚合结果。

### 4.2 RPM（Requests Per Minute）

- 定义：所选窗口内平均每分钟请求数。
- 公式：`RPM = requests / (hours * 60)`。
- 展示：保留 2 位小数；`< 0.01` 显示 `<0.01`。

### 4.3 TPM（Tokens Per Minute）

- 采用区分口径：
  - `inputTpm = inputTokens / (hours * 60)`
  - `outputTpm = outputTokens / (hours * 60)`
  - `totalTpm = inputTpm + outputTpm`（用于总览）
- 说明：缓存命中/写入 Token 继续单独展示，不并入 TPM 主值。
- 展示：千分位缩写（k/M），保留 2 位有效小数。

### 4.4 RPM/TPM 趋势口径

- 趋势粒度：沿用小时桶（`hourly[]`）。
- 每个小时点位计算：
  - `rpmPoint = point.requests / 60`
  - `inputTpmPoint = point.inputTokens / 60`
  - `outputTpmPoint = point.outputTokens / 60`
  - `totalTpmPoint = (point.inputTokens + point.outputTokens) / 60`
- 多选规则场景下按并集聚合后再计算点位速率。

### 4.5 规则多选聚合口径

- 选中多个规则时，所有指标按“并集”聚合（sum）。
- 成功率基于并集聚合：`(requests - errors) / requests`。

## 5. 前端方案

### 5.1 日志页：规则筛选改多选

- 状态变更：
  - `ruleFilter: string` -> `selectedRuleKeys: string[]`。
- 交互细节：
  - 输入框支持关键词过滤选项。
  - 下拉项使用复选框。
  - 已选规则以 Chip 展示，可单个移除。
  - 提供“清空筛选”按钮；清空后等价“全部规则”。
  - 键盘支持：`Enter` 勾选、`Backspace` 删除最后一个 Chip、`Esc` 收起下拉。

### 5.2 日志页：新增 RPM/TPM 卡片

- 请求区新增 `RPM` 摘要卡。
- Token 区新增 `Input TPM` 与 `Output TPM` 摘要卡（可附 `Total TPM`）。
- Tooltip 显示公式与当前分母（例如“基于最近 24 小时”）。

### 5.3 日志页：新增 RPM/TPM 趋势图

- 在现有“消耗趋势”下新增“速率趋势”图表区块。
- 图层建议：
  - `RPM`：折线（右轴，可读性优先）
  - `Input TPM`：折线（左轴）
  - `Output TPM`：折线（左轴）
- 支持图例开关，默认全部开启。
- Tooltip 显示当前小时四个值：`RPM / Input TPM / Output TPM / Total TPM`。

### 5.4 服务页：规则卡片轻量趋势

- 每卡新增 `ruleStatMini` 区域：
  - 使用单图双轴（与统计页表达一致）：
    - `Token`：柱状图
    - `Req`：折线图
- 视觉规范：
  - 请求线：暖色（与日志页请求线保持同家族）。
  - Token 柱：冷色渐变。
  - 高度控制在 32-40px，避免卡片视觉膨胀。
- 响应式：
  - 宽度不足时仅保留数值，隐藏图表。

## 6. 后端与数据结构方案

### 6.1 IPC 与命令参数

- `logs_stats_summary` 入参扩展：
  - `ruleKeys?: string[]`（新）
  - `ruleKey?: string`（兼容保留，逐步废弃）

### 6.2 StatsSummaryResult 扩展

新增字段：

- `ruleKeys?: string[]`
- `rpm: number`
- `inputTpm: number`
- `outputTpm: number`
- `totalTpm: number`

兼容策略：保留 `ruleKey`，前端新版本优先使用 `ruleKeys`。

### 6.3 StatsStore 聚合改造

- `summarize(hours, rule_keys)` 支持规则集合过滤。
- 过滤逻辑：`bucket.rule_id` 命中任一 `ruleKeys` 即纳入聚合。
- 在 summary 输出阶段计算：
  - `rpm = requests / minutes`
  - `inputTpm = input / minutes`
  - `outputTpm = output / minutes`
  - `totalTpm = inputTpm + outputTpm`

### 6.4 规则卡片统计接口

新增命令建议：`logs_stats_rule_cards`。

入参：

- `groupId: string`
- `hours?: number`（默认 24）

返回：

- `rules[]`，每项包含：
  - `groupId`
  - `ruleId`
  - `requests`
  - `tokens`（input+output）
  - `hourly[]`（每小时 requests/tokens）

说明：单次返回一个分组下所有规则，避免前端 N 次请求。

## 7. 性能与稳定性

- 当前统计轮询 3 秒；改为多选后仍维持单次聚合遍历，CPU 增量可控。
- 规则卡片统计建议轮询 10 秒，且仅在 ServicePage 可见时刷新。
- 前端图形渲染使用轻量 SVG Sparkline，不为卡片引入 ECharts 实例。
- 若规则数 > 100，卡片趋势只渲染可视区域（可后续加虚拟列表）。

## 8. 视觉与动效建议

- 多选下拉：选中项使用低饱和强调底色 + 勾选图标，避免高亮过强。
- Chip 动效：进入 120ms fade/slide；删除 100ms shrink。
- Sparkline 动效：首次加载 path draw-in 180ms；后续数据刷新仅过渡，不重绘闪烁。

## 9. 分期落地

### Phase 1（低风险，先交付）

- 多选规则筛选。
- RPM + Input/Output TPM 汇总卡。
- RPM/TPM 趋势图。
- 不改存储结构，仅改聚合接口与前端。

### Phase 2（体验增强）

- 服务页规则卡片轻量趋势图。
- 新增 `logs_stats_rule_cards` 批量接口。

### Phase 3（可选）

- 增加“峰值 RPM/TPM”指标。
- 趋势图支持请求/Token 显示切换。

## 10. 验收标准

- 统计页可同时选中多个规则并正确聚合。
- RPM/Input TPM/Output TPM 在全部规则、单规则、多规则三种场景下可用。
- 统计页新增 RPM/TPM 趋势图，点位与汇总口径一致。
- 服务页每个规则卡片可见请求与 Token 轻量趋势。
- 在 50 条规则规模下，页面交互保持流畅，无明显卡顿。
- 兼容历史数据文件，不触发统计文件迁移失败。

## 11. 需要你确认的口径点

1. 规则卡片趋势默认窗口是否定为“最近 24 小时”？
