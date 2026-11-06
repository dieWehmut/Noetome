# 单个明确扫描页公式重建与语义复核 v1

B06V-T02 的独立交换边界，实际实现和验收状态仍由[账本](../../dev-log/tasks.json)及真实验证记录维护。它尝试识别一个明确选定的扫描式公式区域，保留原区域与原响应，再核对有界等价表达。手工来源声明不进入识别请求，不称为识别结果或恢复的作者原始 TeX。

## 明确入口与来源

```text
noetome knowledge scan-formula <scan-case目录> <api-config.json> <profile.json> <尚不存在的新测试Vault>
```

入口只读取明确选择的 case、api config 和 profile，只创建已有普通父目录下的全新测试根。它不扫描资料目录、不接管生产 Vault、不创建项目数据库或 Release。动词后的 `--` 路径分隔策略沿用既有公式入口。

`case.json` 独立使用 Schema1。全部字段必填，nullable 也不能省略；拒绝未知字段、重复键、不支持版本和未知枚举。

| 字段 | 含义 |
| --- | --- |
| schema_version / purpose | `1` / `scan_formula` |
| selection_id / title | 明确选择身份与显示标题 |
| source_kind | `scanned_page` |
| pdf | `null` 或 `{file, sha256}`，只保留选定归档 PDF 的原始字节 |
| page | `{file, sha256, page, canvas:[W,H]}`，明确整页 PNG 及一基页号 |
| region | `{file, sha256, bounds:[x,y,w,h], legibility}`，明确区域 PNG、像素矩形及 `clear` / `unreadable` |
| semantics | `{file, sha256}`，选定页的手工来源语义声明 |

所有引用均为 case 的普通直接子文件，清单精确等于 manifest 与明确附件。拒绝复用文件名、大小写/Windows 名称碰撞、路径越界、链接、硬链接、重解析祖先和运行期间身份、字节或清单变化。

整页和区域 PNG 各最多 8 MiB，画布两轴不超过 2048，矩形必须为正且在画布内。PNG 完整解码、CRC、实际尺寸和 EOF 必须通过；拒绝动画、尾随字节及只含首部的假文件。Core 将两个解码图像转成同一 RGBA 像素表示，逐行核对区域 PNG 恰等于整页矩形，不能只比较声明的 SHA 或尺寸。解码内存另有 32 MiB 限额。

页号和矩形的实际定位依据是选定整页 PNG 的像素坐标：左上原点，Y 向下。可选归档 PDF 最多 8 MiB，保持原字节；本入口不进行通用 PDF 页面渲染或声称已验证任意 PDF 与 PNG 对应。自造作者用例的 PDF 页数、无文本层、实际渲染与 PNG 的关系由独立验收另外核对，这层证据不能扩大成通用扫描 PDF 支持。

## 手工来源声明与未知

`semantics.json` 为严格 Schema1，producer 固定为 `author_scan_declaration`。字段为 `expression:string|null`、`symbols`、`conditions`、`ranges`、`exceptions`、`uncertainties`；每个 symbol 必填 `{symbol, definition:string|null, unit:string|null}`，uncertainty 为 `{kind:sign|subscript|symbol, description}`。

来源 expression 是手工语义转写，用于在真实识别返回后进行核对。独立 expected 位于运行 case 之外，只由独立校验器读取，不能用于生成识别文本、计划或笔记。符号、定义、单位、前提、范围及例外以来源原值保留；`null`、空数组和未知不同。

原区域不可辨、关键 uncertainty 存在、expression 未知或符号定义未知时，阻断受影响成功表达。识别器即使猜出另一处清晰内容也不能消除这些未知。未知状态按作者来源与原区域证据保留，不用模型自信或正确猜测升级为已核实。

## 一个明确视觉识别方案

最终活动方案为用户已授权的糖猫云 `deepseek-flash` Chat 视觉请求，客户端策略固定为 `tangmao-vision-scan-v1`。服务、模型别名和累计 10M Token 授权沿用[模型服务记录](../requirements/MODEL_SERVICE.md)，不引入另一个自动回退模型。此前固定 pix2tex 的六轮策略、十二次实际调用未获得清晰等价表达，原件、错误响应、代码和环境已封存为失败实验，不作为活动识别能力或成功证据。

锁定范围是明确接口、模型配置别名、请求策略/Schema、客户端代码及实际请求/响应字节。返回 `model` 只记为实际观察；网关底层权重和精确内部模型版本未经独立证实，不能把别名或官方能力页称为可重复的权重版本锁。

`api-config.json` 独立使用严格 Schema1，全部字段必填，拒绝重复键、未知字段及省略 nullable 项：

| 字段 | 含义 |
| --- | --- |
| schema_version / provider | `1` / `openai_chat_completions` |
| transport_mode | `live` 或 `local_http_test` |
| endpoint / model / credential_env | 明确服务端点、配置模型及外部凭据环境变量引用 |
| required_input | `vision` |
| capabilities | 明确 `text`、`vision`、`structured_output` 布尔能力；声明不等于实际调用验收 |
| limits | `max_input_bytes, max_response_bytes, max_output_tokens, timeout_ms` |
| budget | `max_requests` 为 0 或 1，`max_total_tokens` 为本次已核对额度，`max_cost_usd` 必填可空 |

live 只允许已授权的精确 `https://ai.1343263.xyz/v1/chat/completions`、`deepseek-flash` 和 `NOETOME_TANGMAO_API_KEY`。离线 HTTP 测试只允许明确端口的数字环回、人工 `noetome-test-` 模型与测试凭据，真实模型/收费不能冒充本地测试。Core 不读取默认认证文件、不扫描凭据目录，也不把密钥放进请求正文、参数或持久报告。

每次仅一个有界 POST，禁用重定向、代理继承与自动重试。实际 Chat body 使用固定 system 任务/JSON Schema 与 user 内容数组，后者只携带明确原区域 PNG 的内联 base64 图像，`detail=original`。来源手工表达、expected、作者生成源码和建议答案不能进入请求。body 固定 JSON Object 响应、非流式及客户端 thinking 策略；是否真正关闭 reasoning 由原响应记录，不能仅凭客户端配置断言。

只接受单个 stopped assistant 的严格 JSON 识别对象：`schema_version=1, recognized_tex:string|null, uncertainties:[{kind:sign|subscript|symbol,description}]`。HTTP、finish、原始及规范 usage 与模型返回名分别核对，元数据成功不替代表达语义。无法校准的置信保持未知；不能编造一个通过分数。

请求正文最多 12 MiB、响应最多 1 MiB、输出最多 8192 Token，HTTP 请求最多 60 秒，Core 整次运行另有 120 秒控制上限，配置可以进一步收紧。输入工程预留为实际 body UTF-8 字节数加 1024，另预留输出上限，两者合计不得超过已核对额度。价格未知时金额上限不能触发请求，也不能把未知费用填成 0。

累计账本与 DPAPI 复用由窄凭据入口核对：每次先读明确设置和实际当前账本、锁定并保守预留本次配置的总 Token 上限，再调用一次普通 CLI；Core 的正文/输出工程预留必须在此上限内。有效 usage 结算实际使用并返还未用预留，未知用量保持未结清。账本或凭据缺失必须如实停止，不以历史快照重建一个假当前余额，也不重新取得 10M。原始计量记录和用户确认的恢复范围另行记录，不把离线 fixture 的停止额度当成本机余额。

## 等价核对与目标表达

真实识别完成后，Core 解析来源语义与未经修补的识别文本，核对既有有界标量数学结构。负号、变量、关系方向、分子/分母、上下标及运算顺序必须保留。未知宏或语法不能靠原图回退洗成重建成功。

等价规范化只适用于明确定义且重新核对的规则，例如单操作数脚标的 `_i` / `_{i}` 和已核对的分式兼容写法。不能全局删除括号、上横线或其他数学符号，也不能将不同变量按相似字形替换。计划记录具体改写，编译表达必须再次与来源和识别数学结构一致。

profile 沿用[公式契约](FORMULA_FIDELITY_CONTRACT.md)的严格字段及能力语义。只有已验收的 Obsidian 1.6.7 / MathJax 3.2.2 组合可使用原生重建表达。来源和识别语义已核对而目标数学能力不足时，原区域可作为本地图片回退；目标图片能力也不足时阻断。未知应用版本不能仅凭 `verified=true` 成为已测组合。

## 保存、预览和保护

新根 owner 标识本次未发布扫描用途。原 case、来源声明、api config、profile、整页 PNG、选定区域 PNG 和可选 PDF 逐字节保留。实际请求 body、HTTP 收据、原响应、有效 usage 与规范识别对象另外保存；原识别文本与等价编译文本分别标记，不能把派生表达称为作者原始源码。凭据回显时保留安全指纹及省略原因，不持久化泄密原文。

`scan.json` / `scan.md` 表达同一完整事实，包括来源与运行时指纹、区域核对、识别/语义状态、决定与问题、实际产物及逐字段 UTF-8 映射。成功才有 `plan.json`、编译表达或成功预览；阻断保留原件、已收取原响应与问题，不留下成功计划、compiled TeX 或预览。原识别文本若另存，仍只是审计数据，不代表成功表达。

笔记保留定义与单位的实际对应、全部前提/范围/例外、重建或回退说明，以及可追溯的原区域、整页、归档附件和原响应。正文映射必须指向真正的 UTF-8 字节和对应来源字段，指纹或齐全的计划 ID 不能代替实际语义保留。

输入、api config、目录祖先及输出持有能力贯穿调用与收尾重查。运行中出现外部变更时，只清理仍确认属于本次且字节未变的成功对象；保留原件、人工改动、未知文件和不能确认所有权的现场。

输出始终保持 `knowledge_status=pending`、`scientific_quality=not_run`、`published=false`。网络尝试、实际模型调用、有效用量、未知收费与本地 synthetic 测试分别记录；不能沿用失败本地方案的固定零调用报告。实际识别成功和工程语义核对不证明科学正确率、完整资料覆盖、任意扫描公式、多栏或生产发布验收。
