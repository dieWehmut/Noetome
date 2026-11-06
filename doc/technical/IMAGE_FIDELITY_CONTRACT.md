# 明确选定图像保真契约 v1

本契约限定 B06V-T01 的 `noetome knowledge image <case目录> <profile.json> <尚不存在的新测试Vault>`。输入是明确选定的整张 PNG 与作者手工结构声明；命令不识别图像，不恢复作者数据，不扫描资料库，不读取独立 expected。成功仅表示有界工程预览：知识 `pending`、科学质量 `not_run`、`published=false`、网络与收费调用均为零。

## 输入和版本

case 目录恰有三个直接普通文件：`case.json`、它引用的 PNG、它引用的语义 JSON。路径必须是不同的安全 basename，声明 SHA-256 必须与实际文件一致；禁止符号链接、硬链接、reparse、路径越界、额外未选文件和输入/输出碰撞。case、文件、profile 与祖先身份在读入和保存期持有并重验；输出根必须新建，不能接管已有 Vault。

所有 JSON 使用严格 Schema1：拒绝重复键、未知键、缺失必填字段、错误类型和未知枚举。nullable 字段也必须显式存在；布尔不能替代坐标整数。文本作为字面内容保存，XML 特殊字符转义，Markdown 使用能容纳原文字面的围栏。文本布局不能安全保真时保留原图。

`case.json` 必有 `schema_version=1`、`purpose=image_fidelity`、`selection_id`、`title`、`source_kind`、`image`、`semantics`、`dimensions=[width,height]`、`readability`。两个文件引用都是 `{file,sha256}`。source_kind 为 `diagram`、`spatial`、`experimental_curve`、`observation`；readability 为 `clear` 或 `unreadable`。画布是实际选定 PNG 整图像素，不伪造 PDF 页码或区域定位。

PNG 限制为每轴不超过 2048 像素，原文件最多 8 MiB，解码最多 32 MiB。须完整验证 framing、CRC、尺寸、像素解码和 IEND/EOF；动画与尾随内容拒绝，不能从首部尺寸断言原图可用。

`semantics.json` 必有 `schema_version=1`、`producer=author_image_declaration`、`author`、`complete`、`data_available`、`caption`、`conditions`、`exceptions` 和下列数组。它是作者手工声明，`complete=true` 不证明自动识别或科学正确。

| 数组 | 每项必填字段 | 语义 |
| --- | --- | --- |
| elements | id、kind、bbox、label、unit、legend_id | kind 为 rect/circle/unsupported；bbox 为整图整数 x/y/w/h；圆宽高相同 |
| relations | id、from、to、start、end、direction、effect、label、unit、legend_id | direction 为 forward/reverse/none/unknown；effect 为 positive/negative/neutral/unknown；两端坐标绑定 from/to 的边界 |
| spatial_relations | id、a、b、relation | relation 为 left_of/above/inside/overlaps/unknown，必须与实际几何一致 |
| legend | id、label、meaning、color、bbox | color 为明确 #rrggbb；归属引用实际图例身份，不能仅显示相同颜色 |

label、unit、legend_id 以及图例 meaning 为 nullable 必填。`unit=null` 表示来源未给出，`legend_id=null` 表示未声明图例归属，不能推造；关键 label/meaning 为 null 则未知。关系 `label=""` 可以表示明确没有名称；方向和作用仍分别保存，不把无箭头相关关系改为因果。各项身份唯一，引用必须存在，几何必须在整图内。

profile 必有 `schema_version=1`、`app`、`app_version`、`svg_supported`、`local_images`、`verified`。当前 SVG 能力仅限定实际记录的 Obsidian 1.6.7；profile 自报不是实测证据，也不能开启未测目标。

## 决策与有限重绘

决策字段为 `{representation,fallback_reasons,issue_codes}`。representation 是 `svg_redraw`、`original_image` 或 null。文件准入或结构非法在建根前拒绝；合法但未知/不可辨/矛盾输入保存原件和阻断报告，不能生成成功计划或预览。

| 情况 | 结果 |
| --- | --- |
| 关键标签、方向、作用、图例意义、空间关系未知；声明不完整；原图不可辨 | 阻断，representation=null |
| 已知坐标与声明空间关系或边端点矛盾 | 阻断，IMAGE_GEOMETRY_MISMATCH |
| 已知但当前不支持的形状，或文字/交叠布局不能安全保留 | 原图回退，明确原因 |
| 图例填色超出当前固定黑字可核对的保守亮色范围 | 原图回退，IMAGE_COLOR_CONTRAST；保留作者原颜色 |
| experimental_curve 或 observation | 原图保留；data_available 自报也不启用新绘图引擎 |
| 未验目标、未声明 SVG 或 verified=false，但可用本地图像 | 原图回退 |
| 缺本地图像能力 | 阻断，SVG 嵌图也需要本地图像能力 |
| 关键语义已知、几何与布局可核对、目标能力经限定验收 | 有限 SVG 重绘，并完整保留原 PNG |

重绘只生成固定安全 SVG 图元：rect、circle、line、明确箭头 polygon 和转义 text；不消费任意 SVG、脚本、网络资源或自定义宏。viewBox 和图元坐标保持来源整图像素，不缩放重排。固定不透明白画布、黑色关系线/箭头和黑色字面文字避免深色主题吞掉关键内容；有限重绘使用明确编译样式，不宣称与原 PNG 像素相等。箭头 forward 在 end，reverse 在 start，none 无箭头；正负标记、单位、图例颜色与完整图例含义分别保留，正文另保存图例身份归属。画布、图元与文本 UTF-8 映射可追到作者声明的实际字段，不能拿摘要或指纹代替可见含义。

Schema 接受任意明确的 `#rrggbb`；当前有限黑字样式仅重绘所有图例色 RGB 每通道均至少192的保守亮色范围，其余颜色仍是已知语义，保留原图。此范围是有界编译政策，不是科学测量或通用配色正确率；不能偷偷修改作者颜色或把已知颜色改为未知。原生深色背景和自造白字黑底反例都已实际暴露过不可见文字，相关原件与修复结果分别保存。

## 未发布输出

输出是平铺新目录，所有输入副本逐字节保留：`.owner.json`、`source-case.json`、`source-semantics.json`、`profile.json`、`original.png`、`image.json`、`image.md`。owner 限定 producer=knowledge_image、purpose=unpublished_image。两报告保存同一完整事实快照：schema_version、producer、scope=selected_image_fidelity_v1、request_id、status、preview_status、知识/科学/发布状态、零调用计数、case、semantics、profile、checks、decision、plan_fingerprint、inputs、files、artifacts。

checks 分别为 source_preservation、author_declaration、geometry_review、semantic_review、note_plan、rendering、saved_output；描述有界工程检查，不能从它们推定科学通过。成功另存 `plan.json` 和 `图像预览.md`；仅 SVG 决策另存 `redrawn.svg`。笔记同时提供相对本地附件与原图，完整展示标题/说明、元素/单位/图例归属、关系方向/作用/单位、空间、条件和例外。源字段和可见块逐项关联，正文不能只粘一个 JSON 即称为可读表达。

保存后重新读取实际库存、报告、计划、笔记与附件，核对字节/UTF-8 区间及原输入守卫。失败仅撤去本轮确认未被改变的成功结果；原件、已变内容和未知人工文件保留，不通过扩大删除范围掩盖异常。完整通过退出0，语义阻断退出3，准入/Schema 错误退出2，输出错误非零；实际错误代码以严格实现与验证记录为准。

## 独立验收

公开自造[作者素材](../../fixtures/image-fidelity/README.md)在旧 CLI 实际 RED 前冻结整图、标签/关系/单位/图例/空间及独立 expected。CLI 不读取这些预期。[独立校验器](../../scripts/verify_image.py)从冻结原件和预期核对实际 SVG 图元、箭头 polygon、字面文本与单位/图例/空间、正文结构、计划、双报告、附件及映射；同步更新计划、报告、正文与 SHA 的错误仍必须拒绝。

Obsidian 检查使用干净独立配置，保存实际安装/版本、原生图像解码、标签面积、箭头与空间几何、原图/附件点击及最终 CLI 字节联结。SVG 能力探针不是最终输出验收。Windows 完整回归与 Linux 主机限制分别记录，不互相推定；后续扫描重建和跨版本兼容矩阵仍按独立任务验收。
