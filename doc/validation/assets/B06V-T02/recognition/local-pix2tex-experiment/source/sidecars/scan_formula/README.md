# 单个明确选定公式区域的本地识别

本项只接入 [pix2tex 0.1.4 官方 wheel](https://pypi.org/project/pix2tex/0.1.4/) 与 [LaTeX-OCR v0.0.1 固定权重](https://github.com/lukas-blecher/LaTeX-OCR/releases/tag/v0.0.1)。它识别一次明确 PNG 区域，保留实际返回的 TeX；作者声明与独立 expected 不进入识别请求。普通 CLI 不安装依赖、不下载模型、不读剪贴板，也不扫描目录寻找资料。

**当前开发状态**：独立冻结原件的真实识别已经运行，前五个有据策略的清晰样例仍出现错误 overbar；原响应与诊断图完整保留在 [recognition 证据](../../doc/validation/assets/B06V-T02/recognition/)。这份环境与接口说明不构成清晰公式重建验收，也不宣称科学质量或一般 OCR 支持。最终策略和任务状态以 [任务账本](../../dev-log/tasks.json) 与 [扫描契约](../../doc/technical/SCAN_FORMULA_CONTRACT.md) 为准。

## 明确的运行环境

选择 CPython 3.12.14，在新的目录建立专用环境；`requirements-linux-lock.txt` 固定 50 项依赖，Windows 文件固定 52 项，包括 Windows 的 pyreadline3 与 colorama。全部包都有发行物 SHA；只安装二进制 wheel，不使用 GUI、API、训练或额外 OCR 包。安装成功、包 metadata 与 import 实测分别记录，不能由依赖表推定识别成功。

```text
<选定Python> -m venv <新的识别环境>
<新环境Python> -m pip install --require-hashes --only-binary=:all: -r sidecars/scan_formula/requirements-windows-lock.txt
```

Linux 使用对应 `requirements-linux-lock.txt`。PyTorch 和 torchvision 均固定 CPU 发行版；安装时使用官方 PyPI 与 [PyTorch CPU wheel index](https://download.pytorch.org/whl/cpu)。安装是显式准备操作，不是普通识别的自动退路。不得修改既有 PDF 解析环境。

模型目录必须只包含下列四个普通单链接文件，禁止目录或文件链接、额外文件和不匹配的 SHA。模型大二进制不提交 Git，运行前后实际读取及核对；[runtime-lock.json](runtime-lock.json) 记录精确指纹。

| 文件 | 字节数 | SHA256 |
| --- | ---: | --- |
| weights.pth | 102113875 | a63d9141c53d266cb682fb5a8bd83bd5cbe283145e0e78ebdc0f895195a1dfaa |
| image_resizer.pth | 19441973 | 1c3820659985ad142b526490bb25c23d977176ac2073591b3bddada692718458 |
| config.yaml | 917 | a9b18acf6718ffed79ab669c9957c46936714f4b983705f0b1361d90f4ea89aa |
| tokenizer.json | 24174 | 1dc27b18d6a518d0d5ff3f4bb7bd98521fe80ad39e5b2a246d4109f1bb9d5019 |

权重下载地址为官方 release 的 [weights.pth](https://github.com/lukas-blecher/LaTeX-OCR/releases/download/v0.0.1/weights.pth) 与 [image_resizer.pth](https://github.com/lukas-blecher/LaTeX-OCR/releases/download/v0.0.1/image_resizer.pth)。配置和 tokenizer 使用上述 0.1.4 wheel 内 `pix2tex/model/settings/config.yaml`、`pix2tex/model/dataset/tokenizer.json` 原字节；本机路径不写死进仓库。明确下载或复制后先核对字节，再选择该目录作为 `NOETOME_SCAN_MODEL_DIR`；解释器通过 `NOETOME_SCAN_PYTHON` 明确指定。

## 嵌入代码与实际识别

可审阅的代码是 `adapter.py`。运行 `python scripts/build_scan_formula_sidecar.py` 生成 `adapter_payload.py` 与 SHA 元数据；提交前运行同工具的 `--check`。Rust 消费可信短 wrapper，解压后断言源字节 SHA 和长度，再执行同一源码。Python 使用 `-I -B -X utf8`，不接受资料指定代码、解释器、命令行或故障环境变量。

请求只有一次选定 PNG 的绝对路径、SHA、字节数、模型目录、请求身份和资源限额。原文件完整保留，响应中的 `recognized_tex` 是真实未经修补的返回；支持范围外的命令、错误负号/下标、未核实符号和未知可辨性应由 Core 阻断。置信度保持 `null`，模型不提供本项已校准的概率；不以模型返回字符串替代独立语义核对。

pix2tex 的默认构造可能下载缺失权重、默认预测尝试写剪贴板；适配器在导入/构造前核对实际文件，禁用下载函数、socket 连接和剪贴板写入，强制 `torch.load(weights_only=True)`。这属于有界运行限制，并非操作系统文件/网络沙盒证明。

## 许可与能力依据

pix2tex 项目和 wheel 附带 MIT 许可，原版权与全文保留在 [pix2tex-MIT.txt](licenses/pix2tex-MIT.txt)。使用或修改上游预处理与调用逻辑时保留该 notice；不改变 Noetome 整仓库许可证，不分发第三方依赖二进制。将来打包依赖需保留各 wheel 自带全部许可。

2026-11-06 实际读取的 [DeepSeek 官方模型页](https://api-docs.deepseek.com/quick_start/pricing) 与 [Vision guide](https://api-docs.deepseek.com/guides/vision) 已明确 deepseek-flash 支持图像；不能将旧文本验收当成当前官方不支持视觉。此次仍只选用固定本地方案，糖猫云网关视觉、实际内部路由版本和费用均未实测，不调用服务或读取凭据，累计预算保持不变。原网页指纹与选择理由见 [official-capability-check.json](../../doc/validation/assets/B06V-T02/recognition/official-capability-check.json)。
