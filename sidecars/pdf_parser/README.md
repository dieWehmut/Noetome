# 单一文本 PDF adapter

使用 [pdfplumber v0.11.9 官方接口](https://github.com/jsvine/pdfplumber/tree/v0.11.9)：从其 pdfminer.six 字符清单取得正文与布局包络，按固定 CropBox/旋转与行规则交付 [PDF 沙盒契约](../../doc/technical/PDF_PARSE_CONTRACT.md)。只有这个文本解析 adapter；pdfminer 是它的依赖，pypdfium2/Pillow 是上游所需依赖，不代表 Noetome 增加第二个文本解析后端。图片/表格/公式/扫描仍需复核。

可读 `adapter.py` 经 `scripts/build_pdf_sidecar.py` 生成字节绑定的 `adapter.zlib` 与 SHA 元数据，Rust `include_bytes!` 嵌入 payload。开发者可明确选定可信解释器 `NOETOME_PDF_PYTHON`；未设置时 Windows 使用当前程序相邻的 `pdf-python/python.exe`，两者均检查绝对路径、路径组件和普通文件。明确无效的覆盖值不会回退，不搜索 PATH 或资料目录；普通 CLI 没有 adapter/fake 参数。原生 argv 使用 Python `-I -B -X utf8 -c` 和标准库 zlib/compile/exec 运行可信代码，忽略用户 Python path、禁止字节码写入；PDF 只从已确认完整原始字节的 BytesIO 解析。资料不决定可执行文件、代码、输出文件名或网络操作。

原始可读源码的 Windows `-c` 命令行曾实测超出 CreateProcess 限额而失败；压缩 wrapper 留出路径/参数空间，不要求源码难以审阅。修改 adapter 后先 `python scripts/build_pdf_sidecar.py`，提交前 `python scripts/build_pdf_sidecar.py --check`；生成代码解压必须与源码完全一致。普通 Rust 编译/fake 回归不启动 Python 或 PDF 库，只消费已提交 payload。

## 建立环境

选择 CPython 3.12.14 Windows x64，在一个新测试目录创建 venv；将下面 `python` 换成本轮实际选定解释器，不将开发机路径写入脚本：

```powershell
python -m venv <新环境目录>
<新环境目录>/Scripts/python.exe -m pip install --require-hashes --only-binary=:all: -r sidecars/pdf_parser/requirements-lock.txt
$env:NOETOME_PDF_PYTHON = '<新环境目录的绝对路径>/Scripts/python.exe'
./target/debug/noetome.exe source parse-pdf <选定PDF> <尚不存在的测试根>
./target/debug/noetome.exe source inspect-pdf <测试根>
```

`requirements-lock.txt` 精确锁定全部运行依赖及官方 wheel SHA；`runtime-lock.json` 记录已选平台与版本。adapter 在导入前核对 Python 与包版本，失配明确失败。官方 pdfplumber `Requires-Python >=3.8`、锁定的 pdfminer `>=3.10`，不等于全部 Python 平台已由本项目测试；本次只记录 3.12.14 / Windows x64。

官方核对依据：[pdfplumber requirements](https://github.com/jsvine/pdfplumber/blob/v0.11.9/requirements.txt)、[坐标/字符/文本 API](https://github.com/jsvine/pdfplumber/blob/v0.11.9/README.md)、[pdfminer.six](https://github.com/pdfminer/pdfminer.six)、各精确版本的 `https://pypi.org/pypi/<包>/<版本>/json`。这些资料说明接口/依赖；实际支持由独立 fixture 与 CLI 验收证明。

## 依赖许可

| 包 | 锁定许可 | 官方依据 |
| --- | --- | --- |
| pdfplumber | MIT | [v0.11.9 LICENSE.txt](https://github.com/jsvine/pdfplumber/blob/v0.11.9/LICENSE.txt)，副本在 licenses/ |
| pdfminer.six | MIT | [官方 LICENSE](https://github.com/pdfminer/pdfminer.six/blob/master/LICENSE)，安装包元数据 |
| pypdfium2 | BSD-3-Clause / Apache-2.0 及 PDFium 依赖许可 | [官方 licensing](https://pypdfium2.readthedocs.io/en/stable/readme.html#licensing)，分发 binary 时保留 wheel 所带 notices |
| Pillow | MIT-CMU | [官方 LICENSE](https://github.com/python-pillow/Pillow/blob/main/LICENSE) |
| charset-normalizer | MIT | [官方 LICENSE](https://github.com/jawah/charset_normalizer/blob/master/LICENSE) |
| cryptography | Apache-2.0 OR BSD-3-Clause | [官方 LICENSE](https://github.com/pyca/cryptography/blob/main/LICENSE) |
| cffi | MIT-0 | [官方项目](https://cffi.readthedocs.io/)，精确发行版元数据 |
| pycparser | BSD-3-Clause | [官方 LICENSE](https://github.com/eliben/pycparser/blob/main/LICENSE) |

这里不分发依赖二进制，也不修改 Noetome 的仓库许可证。后续打包完整运行时须保留各 wheel 的全部许可证/依赖 notices，不能用这一张表替代。上游许可与技术说明不是科学正确率或安全沙盒证明。

## 验证入口

固定输入在 `fixtures/pdf-text-v1/`。adapter Python 测试通过标准 unittest 调用实际子进程；Rust `tests/pdf_parse.rs` 验证普通 CLI、完整正文/页/位置、语义篡改与只读沙盒。普通 sidecar fake 回归不导入这组依赖，不自动联网安装。

只对实测小型单栏正文声明 parsed；未知空白、扫描、图形/图片、公式、多栏、旋转阅读顺序等 needs_review，损坏/加密和部分页异常 failed。原始原文留作定位依据，不生成来源版本、IR、Vault 或科学/长任务指标。实测支持、版本和命令详见 B03-T02 的验证记录。
