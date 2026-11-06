# B06V-T03 文档、链接和原字节保护独立终审

本轮只读审查了验收记录、视觉能力标准、公式/流程/扫描/图像四契约、执行方案及仓库 README。98 个本地 Markdown 文件/目录链接全部存在；检查未获取外部 URL 内容。所读八份文档前后 SHA/长度一致，记录见 [final-doc-link-attributes-audit.json](final-doc-link-attributes-audit.json)。该记录保留当时文档版本，最后正在执行的回归不在本审查提前计为通过。

四契约及执行方案中的精确版本配对、MathJax 实际 4.1.3、Pele 1.8.0 与显式未知 Mermaid 版本、strict Schema 1/2、原图回退与关键未知阻断、单独标记的手写阅读探针均与已审源码和原生原件一致。64 个唯一组合、60 产品捕获/4 手写探针、812 个来源原文件及当前 SVG 完整 RGBA/真实标签差分已按独立原件审查成立。Windows Rust 726/0/0 和当前 1,825 项依赖/最终 PE 的绑定已从固定原 stdout/stderr 和 source ZIP 独立重算；未运行完整 Linux Rust。

已向父代理报告四处需按最终实际结果维护的表达，父代理独占标准与验收记录的修改：

1. 旧 19 CLI ELF 的二进制及 fixture/checker 指纹不包含编译时 Rust 源前后锚，不能标作最终 build-source 绑定。旧记录保留，新 Windows C PE/源码强锚下的 19 项专项在该轮仍待完成。
2. 前一 Linux 380 项完整 Python 通过后，FlowTargetCliTests 又进行真实本机 import 设置修复；测试文件最终 SHA 为 `42b63646f045fd15c75c43cbbb48a58f7b6394b4665fcde92be95e7e4c6e54c5`。新两平台完整回归应按其新源快照实际结果填写，不能把旧整文件指纹宣传为最新相等。
3. Windows Python 失败历史应分别记录漏带旧 collector 的 FileNotFound、Core 正确拒绝旧 Linux SQLite import_path，以及由普通 CLI 在新目录重建相同作者来源项目的测试设置修复。它们不应合并描述为一般环境问题，也不应省略旧 19 ELF 的源码锚缺口。
4. 视觉标准关于公式/扫描的判断顺序宜明确为先核对各自来源/识别表达及关键未知，再选择精确目标能力；与实际 Core 先保留语义阻断再决定目标回退的顺序一致。

`.gitattributes` 的新任务资产与三个冻结包规则通过逐路径检查：2,240 个文件全部 `text=unset`，无启用的 filter 或 working-tree-encoding。另选择十份实际含 CRLF 的原件，用只读 `git hash-object --stdin --path` 和 `--no-filters` 比较，blob SHA 均相同；没有写入 Git object 或索引。基础 `-text` 规则覆盖整个任务资产树及冻结包，ZIP/bin/分段二进制的附加规则不引入换行转换。源、作者 expected、Windows stdout/stderr 和原 Markdown 可按原字节提交保存。

最终普通 CLI 专项与两平台完整 Python 收据由父代理/对应执行者完成后追加。已有源码、原生矩阵、失败历史和作用域审查全部保留，不用文档、账本、准备成功或 Git 状态替代这些检查。

## 后续收据读回（Windows最终源）

最终Windows完整Python原件已独立读回，见 [final-windows-python-source-readback.json](final-windows-python-source-readback.json)：原stderr实际434个通过行、7个跳过行，总441项，退出0；三个FlowTargetCliTests均实际通过。archive receipt列出的十份原文件SHA/长度相等，source ZIP的1,971项源文件及附带清单全部CRC/SHA相等；source-before、source-after和原工作区清单完全相等。最终prepare为9c28fd、流程测试为42b636的完整指纹在该原ZIP中成立。读取时当前树相对于此完整快照只变更了`doc/technical/VISUAL_COMPATIBILITY.md`文案，产品、脚本、测试和冻结输入全部逐字相同。视觉标准已明确先来源/识别/关键未知、再精确目标选择。

旧能力专项的编译来源缺口准确覆盖数学/流程19次、扫描4次、图像7次的阶段ELF二进制收据：当时保存二进制及fixture/checker指纹，但未保存实际编译Rust源前后锚。它与Windows完整Python首轮10个缺collector错误头和一个import_path错误、第二轮单一import_path错误分别属于不同证据范围；没有所谓source bitmap错误。新Windows C锚定的30项、15项普通项目设置及完整后置拒绝检查仍等其新公开原件独立读回；本段不预先计入。

## 最后公开原件与文档收束

最后独立读回见 [final-capability-regression-readback.json](final-capability-regression-readback.json)，可用 [run-final-capability-regression-readback.py](run-final-capability-regression-readback.py) 从固定原包重算。WindowsC新专项两个ZIP共2,759成员、Linux04两个ZIP共15,110成员，合计17,869成员全部CRC/SHA/长度成立。WindowsC完整1,915编译原件可从运行源和两份原非Rust Python变体准确恢复；1,825个Rust依赖与当前树逐字相同。原actual build stdout/stderr/snapshot指纹及25,939,968字节e42c PE均直接相等。当前独立Python源码、作者和冻结输入另外按真实字节核对；原归档生成缓存全部验CRC/SHA，当前工作区暖`__pycache__`不等同源码，也不用于当前源码相等声明。

新30主命令加15额外设置的45次实际CLI逐stdout/stderr/receipt核对；27领域场景、13个原生绑定案中的40份文件（16份Markdown和24份相关资源）逐字等于原生捕获的冻结输出，四份扫描response.bin另外逐字相等。7类实质拒绝和2次初步结构拒绝分别保存；未知数学与SVG目标的最终伪造使用真实decision字段且全部普通hash/双报告同步，拒绝来自source-grounded representation。原图节点判据最终只把“签名完整”改成“签名缺失”，原文件名和把详情条件当图条件的失败尝试保持历史，未改作者/expected或重发已经完成的CLI/HTTP。旧ELF的缺编译源锚由这次新的WindowsC证明补齐；未推断旧ELF编译来源。

Linux第四最终快照实际380项=361通过+19平台跳过，0失败/错误，原stderr175.940秒。1,973项运行输入（1,971源及两份ELF）、源前后和相关工作区前后清单均相等；最终9c28fd prepare及42b636 flow test字节成立。Windows第三最终快照441项=434通过+7权限跳过及此前两轮11/1错误分别保存。上述记录仍不包含完整Linux Rust或未测平台原生渲染。

[final-document-closeout-audit.json](final-document-closeout-audit.json)保存最后11份文档及135个本地路径链接，全部存在且读前后文档字节一致；当时2,309个任务资产/三冻结包文件全部Git text=unset且无filter/encoding，十份实际CRLF样本clean前后blob相等。主验收记录、视觉标准、四契约、执行计划及新专项README中的计数、准确版本/未知、失败历史及有限范围与原件一致，未发现剩余事实或本地链接错误。verification-summary.json九份原件引用SHA/长度另全核对，其64原生、726Rust、441/380Python及45CLI/7+2拒绝汇总与独立原件相符。此前四项文档/证据作用域发现至此均有明确修正与后续原件；普通Git提交、原字节索引审查及远端核对由父代理收尾。
