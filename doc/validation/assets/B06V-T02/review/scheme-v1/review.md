# B06V-T02 本轮识别方案与扫描语义只读审查

结论：清晰扫描公式重建仍未验收。前五个固定pix2tex策略都没有保持冻结作者数学语义；不能删除bar、换expected、换成更简单作者case，或以正确阻断/原图回退替代清晰重建。先执行父代理已经授权的第六个固定细节保护策略；若仍错，建议终止本地识别作为活动方案，将用户已授权的同一糖猫Chat端点与deepseek-flash视觉请求明确选作唯一最终方案，保留所有pix2tex失败历史。

## 真实依据与有限修复

本轮重读33文件作者范围、clear expected以及5轮clear原响应。冻结原区域是560×200、x_i = -\frac{a_i}{b_i}；第1/4/5轮原文本额外识别bar{a_i}，第2轮把x_i识成kappa_s，第3轮成为其他数组/求和表达。extra bar改变分子含义，第2/3轮也不是等价表达。

实看原PNG和第5轮实际派生PNG；第5轮trace显示initialpad384×160，resizer逐步推荐256、160、96，最终张量1×1×64×96。这个信息丢失风险与误识别有具体像素依据；本轮未证明它是唯一误识别原因。

父代理新增统一recommended_width至少initial_width/2的保护可作为一次有据算法候选：调用前固定通用规则、代码和阈值，所有选定状态执行同一策略，保存实际建议宽度/钳制宽度、原/派生PNG及tensor/policy/源码指纹。2倍限制是保守工程政策，不是独立证明每个glyph都会保真，更不能因此自动通过语义核对。本轮尚未观察第六轮实际效果；不得按gold遍历倍率、在多个结果间挑答案或把修补文本记为原响应。

## 替换成同一已授权视觉接口

本轮通过公开HTTP实际重取DeepSeek视觉与能力文档，均HTTP200，原HTML/URL/时间/SHA见official-docs.json。官方当前声明deepseek-flash可接受图像、Chat用户content数组可含base64 image_url；detail=original/high保留输入尺寸。官方能力页所列DeepSeek-V4.1-Flash-1017只属于官方服务当前声明。文档不能证明糖猫实际路由、model权重版本或实际收费。

推断：同一用户授权服务https://ai.1343263.xyz/v1/chat/completions、同一deepseek-flash别名、既有凭据与10M累计预算内的一次有界视觉请求，符合当前任务明确选择图像并核实模型能力的范围。可替换未通过本地方案；最终普通CLI只保留一个活动识别路径，禁止pix2tex失败后自动再请求Vision或反向fallback。

可锁定的是最终adapter/HTTP实现、接口与配置别名、请求格式、提示/policy、已核实官方文档快照和每次真正发送的图像/正文指纹。回包model字段必须保存为实际观察；网关底层版本和权重无法独立证实时保持unverified，不把别名或官方能力表冒称精确模型权重锁定。若报告使用“版本锁定”，必须明确此接口/策略锁定边界和底层未核实限制。换服务或新模型族不从本次建议获得授权。

当前MODEL_SERVICE要求每次先核对本机实际累计usage与pending预留，发起前预留，返回有效usage再结算所有输入/输出/reasoning；缺用量或不明确完成不能记0，也不能自动返还预留。必须保持单请求、无自动重试/批量；价格未知不借用官方费率推算糖猫账单。只上传明确选择的公开原区域和通用转写要求，不发送手工expression、作者生成源码、expected或建议答案。

先做clear的一次真实请求能力/语义验收，保存完整HTTP/原response/实际model/usage/finish_reason/请求体与输入SHA；成功服务路由和正确表达必须从实际返回证明。然后在最终普通CLI同一方案实跑clear、能力原图回退与两个缺损状态。缺损页即使Vision猜出清晰答案仍按作者legibility/uncertainties阻断；原request和未经修补response必须留存，不能把手工groundtruth包装成模型输出。最终必须完成独立数学结构/上下标/负号/分子分母、原件/报告/映射、同步篡改、资源保护、干净Obsidian与完整回归。

## 当前scan实现观察与收束要点

当前读取/保存代码正确分离OCR图像请求与手工语义：OCR wire仅区域路径/字节指纹、固定模型目录和limits。scan_input完整PNG验证后统一RGBA，逐行核对region严格等于page声明矩形；PDF只保留有界字节并验头，不冒称通用PDF页/PNG对应。语义评价先保留来源不可辨/uncertainty/unknown，再解析来源和未改识别表达；仅单scalar脚标括号等有限重写，不允许删bar。未知阻断即使模型猜对也不会洗成成功。保存raw-response/diagnostic/recognized_tex与编译表达分开，成功计划与原件、字段映射可追溯；这些源码观察不替代实际CLI/守卫测试。

合并前存在一个具体策略漂移：本轮AST读取的活动adapter POLICY没有decoding，predict仍直接调用默认engine(image,resize=True)；scan_model的RecognitionPolicy却必填decoding，scan_recognition要求greedy_argmax_v1。第1轮真实原response同样缺decoding；第5轮response另有preprocessing，而当前Rust该DTOdeny_unknown_fields没有该字段。当前本地源码/可信wire若未经同步，不能直接消费那些试验response或宣称实际greedy/aspect执行。保留本地方案时需同步真正adapter、嵌入源码/payload、严格DTO及策略指纹；若选择Vision，替换活动协议/producer/model/usage字段并保留旧记录。synthetic guard通过不能替代最终adapter普通运行。

现有invoke_api_credential.ps1只启动extract api，不能原样证明scan凭据调用。可做窄入口复用，仍保持已选DPAPI引用、内存解密、只给子进程环境、不把key放参数/正文/日志/明文文件。Vision活动实现同时应改真实network_calls/paid_calls/报告壳；当前scan固定“本地识别，网络/收费0”只适用于本地路径，不能沿用到付费视觉调用。

本轮未修改产品/脚本/标准/作者/账本、未commit，未读凭据或发模型/付费请求。实际执行是源码与原响应读取、AST字段比较、两张PNG视觉检查、公开官方文档HTTP2次；没有运行Rust/Python测试或重新调用pix2tex。具体pin及观察见review-receipt.json。不推进B06V-T03，不以本次方案建议complete。

收束时父代理告知：第六个通用downsampling guard的clear实际响应仍错，已经决定保留本地失败历史、将同一已授权Vision作为唯一最终活动方案。此新决定符合上述有界替换建议；本轮没有审尚未存在的新API实现，稳定新协议与真实请求成功仍需后续独立审查。
